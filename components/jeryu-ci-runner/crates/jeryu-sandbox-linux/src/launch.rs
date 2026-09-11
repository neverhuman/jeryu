//! The unsafe island: apply the kernel sandbox primitives to a child process
//! via `pre_exec`, FAIL-CLOSED on any required-but-failed primitive.
//!
//! Ordering inside the child (between `fork` and `execvp`) matters:
//!
//! 1. `setpgid(0, 0)` so the watchdog can group-kill the whole subtree.
//! 2. join the cgroup subtree (write our pid) so limits bind before exec.
//! 3. `PR_SET_NO_NEW_PRIVS` (always, non-negotiable).
//! 4. unshare namespaces *only where the kernel allows* (degraded-skip here).
//! 5. Landlock workspace-only-writable ruleset (when ABI present).
//! 6. seccomp default-deny-with-allowlist (last, so our own setup syscalls run).
//!
//! Anything required by the resolved [`EnforcementLevel`] that fails is turned
//! into an `Err` from `pre_exec`, which aborts the spawn — we never exec the job
//! with a half-applied sandbox.

// The unsafe surface is confined to this module. The crate-level lints table
// (in Cargo.toml) allows unsafe_code; this inner attribute documents the scope.
#![allow(unsafe_code)]

mod cgroup;
mod internals;
mod pty;
mod report;

use internals::{apply_in_child, build_payload};
use pty::wire_pty_slave;
use report::classify;

use crate::capability::{EnforcementLevel, SandboxCapabilities};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::sandbox::{LandlockRule, SandboxPlan};
use std::collections::BTreeMap;
use std::io::Error as IoError;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Error raised when the sandbox cannot be applied or the process cannot start.
#[derive(Debug)]
pub struct SandboxError {
    code: &'static str,
    message: String,
}

impl SandboxError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Machine-readable code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SandboxError {}

/// Result alias for sandbox launch operations.
pub type SandboxResult<T> = Result<T, SandboxError>;

/// What the sandbox actually applied vs. honestly skipped, plus the post-exec
/// proof read from `/proc/<pid>/status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnforcementReport {
    /// Resolved enforcement level for this run.
    pub level: String,
    /// Primitives that were applied.
    pub applied: Vec<String>,
    /// Primitives that were honestly skipped (kernel unavailable).
    pub skipped: Vec<String>,
    /// `NoNewPrivs` bit read from `/proc/<pid>/status` (1 == enforced).
    pub proc_no_new_privs: Option<u8>,
    /// `Seccomp` mode read from `/proc/<pid>/status` (2 == filter mode).
    pub proc_seccomp: Option<u8>,
}

impl EnforcementReport {
    /// Deterministic JSON for the enforcement.json artifact (no serde dep).
    pub fn to_json(&self) -> String {
        let join = |items: &[String]| {
            items
                .iter()
                .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(",")
        };
        let opt = |v: Option<u8>| {
            v.map(|n| n.to_string())
                .unwrap_or_else(|| "null".to_string())
        };
        format!(
            concat!(
                "{{",
                "\"level\":\"{}\",",
                "\"applied\":[{}],",
                "\"skipped\":[{}],",
                "\"proc_no_new_privs\":{},",
                "\"proc_seccomp\":{}",
                "}}"
            ),
            self.level,
            join(&self.applied),
            join(&self.skipped),
            opt(self.proc_no_new_privs),
            opt(self.proc_seccomp),
        )
    }
}

/// Sandbox payload with parent-prepared Landlock and seccomp state. The
/// compatibility cgroup path write still allocates during child setup.
struct SandboxPayload {
    cgroup_procs: Option<PathBuf>,
    apply_user_ns: bool,
    apply_mount_ns: bool,
    apply_pid_ns: bool,
    landlock: Option<OwnedFd>,
    seccomp_bpf: Option<seccompiler::BpfProgram>,
    /// Slave end of an allocated PTY to become the child's controlling terminal
    /// (stdin/stdout/stderr). `None` keeps the default piped/null stdio.
    pty_slave_fd: Option<RawFd>,
    /// Best-effort `setrlimit` fallback values. These are a backstop only: the
    /// real protection for agent jobs is the fail-closed cgroup gate in
    /// [`crate::capability`].
    rlimits: RlimitFallback,
}

/// Plain limits copied out of the plan for the `setrlimit` fallback inside the
/// forked child (which must not touch the allocating `CgroupLimits` type).
#[derive(Clone, Copy)]
struct RlimitFallback {
    memory_max_bytes: u64,
}

/// Spawn `job`'s command under the sandbox described by `plan`, given the probed
/// `caps`. Returns the live [`Child`] (group leader, stdout/stderr piped) for
/// the watchdog to supervise.
///
/// The function refuses to spawn when the enforcement level is `Unavailable`
/// (cannot fail closed). Under `Degraded`, the missing primitives are recorded
/// and skipped; everything still-available is applied fail-closed.
/// How the sandboxed child's stdio is wired.
pub enum ChildIo {
    /// stdin = `/dev/null`, stdout/stderr = pipes (the default the watchdog drains).
    Piped,
    /// All three standard fds are wired to `slave_fd`, the slave end of a PTY
    /// allocated by [`open_pty`]; the child becomes a session leader with that
    /// PTY as its controlling terminal. The caller keeps the master end to read
    /// agent output and write control input, and should close its own copy of
    /// the slave after the spawn returns.
    Pty {
        /// Slave PTY fd (still owned by the caller).
        slave_fd: RawFd,
    },
}

/// Allocate a PTY pair, returning `(master, slave)` owned fds. The master is
/// driven by the supervisor; the slave is handed to [`spawn_sandboxed_with_io`]
/// via [`ChildIo::Pty`]. The master is set close-on-exec so the execed child
/// never keeps it open.
pub fn open_pty() -> SandboxResult<(OwnedFd, OwnedFd)> {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    // SAFETY: openpty fills the two fd out-params; the name/termios/winsize
    // out-params are null. On success (0) both fds are valid and owned here.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if rc != 0 {
        return Err(SandboxError::new(
            "pty_alloc_failed",
            IoError::last_os_error().to_string(),
        ));
    }
    // SAFETY: openpty returned 0, so master is a valid fd we exclusively own.
    unsafe {
        let flags = libc::fcntl(master, libc::F_GETFD);
        if flags != -1 {
            let _ = libc::fcntl(master, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }
    // SAFETY: both fds are valid and exclusively owned after a successful openpty.
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    Ok((master, slave))
}

/// A signal to deliver to a sandboxed child's whole process group.
pub enum GroupSignal {
    /// `SIGINT` (Ctrl-C): ask the agent to interrupt.
    Interrupt,
    /// `SIGTERM`: ask the agent to terminate.
    Terminate,
    /// `SIGKILL`: force-kill.
    Kill,
}

/// Deliver `signal` to the process GROUP led by `leader_pid`. The sandboxed
/// child is a group/session leader (`setpgid`/`setsid` in `pre_exec`), so this
/// reaps its descendants too. A failure (e.g. the group already exited) is
/// ignored — the caller is tearing down regardless.
pub fn signal_group(leader_pid: u32, signal: GroupSignal) {
    let sig = match signal {
        GroupSignal::Interrupt => libc::SIGINT,
        GroupSignal::Terminate => libc::SIGTERM,
        GroupSignal::Kill => libc::SIGKILL,
    };
    // SAFETY: kill() with a negative pid targets the process group; no pointer
    // args, no shared state. A non-existent group is a benign error we ignore.
    if let Ok(pid) = i32::try_from(leader_pid) {
        unsafe {
            let _ = libc::kill(-pid, sig);
        }
    }
}

/// Resize the PTY whose master is `master_fd` to `rows` x `cols`.
pub fn resize_pty(master_fd: RawFd, rows: u16, cols: u16) -> SandboxResult<()> {
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCSWINSZ reads a winsize struct through the pointer; `master_fd`
    // is a valid PTY master fd owned by the caller.
    let rc = unsafe { libc::ioctl(master_fd, libc::TIOCSWINSZ, &ws) };
    if rc != 0 {
        return Err(SandboxError::new(
            "pty_resize_failed",
            IoError::last_os_error().to_string(),
        ));
    }
    Ok(())
}

/// Spawn `job`'s command under the sandbox with the default piped stdio. See
/// [`spawn_sandboxed_with_io`] for the PTY variant.
pub fn spawn_sandboxed(
    job: &JobRequest,
    plan: &SandboxPlan,
    caps: &SandboxCapabilities,
    env: &BTreeMap<String, String>,
) -> SandboxResult<Child> {
    spawn_sandboxed_with_io(job, plan, caps, env, ChildIo::Piped)
}

/// Spawn `job`'s command under the sandbox, choosing the child's stdio wiring.
/// `ChildIo::Piped` is byte-identical to [`spawn_sandboxed`]; `ChildIo::Pty`
/// makes the given PTY slave the child's controlling terminal (the kernel
/// confinement is applied identically either way).
pub fn spawn_sandboxed_with_io(
    job: &JobRequest,
    plan: &SandboxPlan,
    caps: &SandboxCapabilities,
    env: &BTreeMap<String, String>,
    io: ChildIo,
) -> SandboxResult<Child> {
    let level = caps.enforcement_level(plan);
    if let EnforcementLevel::Unavailable { reason } = &level {
        return Err(SandboxError::new("sandbox_unavailable", reason.clone()));
    }

    let mut payload = build_payload(plan, caps, &job.workspace)?;
    let cgroup_cleanup = payload.cgroup_procs.clone();

    let mut cmd = Command::new(&job.command);
    cmd.args(&job.args)
        .current_dir(&job.workspace)
        .env_clear()
        .envs(env);

    match io {
        ChildIo::Piped => {
            cmd.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        }
        ChildIo::Pty { slave_fd } => {
            // The child wires its own 0/1/2 to the slave in pre_exec (after
            // setsid + TIOCSCTTY), so we leave std's stdio inherited and override
            // there — keeping the dup2 ordering under our control regardless of
            // std's internal stdio sequencing.
            payload.pty_slave_fd = Some(slave_fd);
        }
    }

    // Landlock construction and path lookup have finished in the parent.
    // Existing cgroup path writes still allocate here, so this correction alone
    // does not establish allocation-free setup for the entire child sequence.
    // Any setup error makes spawn fail closed before the job is exec'd.
    // SAFETY: pre_exec runs only in the forked child between fork and exec.
    // The payload is already prepared; the closure returns Err to fail spawn.
    unsafe {
        cmd.pre_exec(move || apply_in_child(&payload));
    }

    match cmd.spawn() {
        Ok(child) => Ok(child),
        Err(err) => {
            // Best-effort: remove the cgroup we created if exec never happened.
            if let Some(dir) = cgroup_cleanup.as_deref().and_then(std::path::Path::parent) {
                let _ = std::fs::remove_dir(dir);
            }
            Err(SandboxError::new(
                "process_start_failed",
                format!("{err} (sandbox setup may have failed closed)"),
            ))
        }
    }
}

/// Spawn an arbitrary HOST command on a PTY, WITHOUT the kernel sandbox.
///
/// This is the launch path for the docker-backed agent runtime: on a host whose
/// AppArmor blocks the unprivileged-userns sandbox, the container engine — not the
/// host process — is what confines the agent, so the `docker run ...` invocation
/// runs as an ordinary child. It still gets a real controlling terminal (the same
/// `setsid` + `TIOCSCTTY` + `dup2` wiring [`spawn_sandboxed_with_io`]'s PTY path
/// uses) so the agent CLI inside the container renders to a TTY, and it leads its
/// own session/process group so [`signal_group`] reaps the whole tree.
///
/// `program`/`args`/`env` are the host command (e.g. `docker run ...`). `cwd` is
/// where the host process runs; the container's own `-w /workspace` governs the
/// agent's cwd. The caller keeps the PTY master to read output and write input and
/// must drop its own copy of `slave` after this returns.
pub fn spawn_command_on_pty(
    program: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    cwd: &Path,
    slave: &OwnedFd,
) -> SandboxResult<Child> {
    let slave_fd = slave.as_raw_fd();
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd).env_clear().envs(env);
    // The closure runs in the forked child between fork() and exec(). It only
    // performs direct, async-signal-safe syscalls (setsid, ioctl, dup2, close);
    // it mutates no parent allocator state. Any error fails the spawn closed
    // before exec, so the child never runs with a half-wired terminal.
    // SAFETY: pre_exec only runs the async-signal-safe terminal wiring above.
    unsafe {
        cmd.pre_exec(move || {
            // Lead a new session so TIOCSCTTY can claim the slave as our tty.
            if libc::setsid() == -1 {
                return Err(IoError::last_os_error());
            }
            wire_pty_slave(slave_fd)
        });
    }
    cmd.spawn()
        .map_err(|err| SandboxError::new("process_start_failed", err.to_string()))
}

#[cfg(test)]
pub(crate) fn terminate_prepared_child(code: i32) -> ! {
    // SAFETY: the caller is a forked child that must not unwind or run Drop.
    unsafe { libc::_exit(code) }
}

/// Read `/proc/<pid>/status` to PROVE the kernel actually enforced the sandbox.
/// `NoNewPrivs:1` and `Seccomp:2` are the load-bearing lines.
pub fn verify_enforcement(pid: u32, level: &EnforcementLevel) -> EnforcementReport {
    let (applied, skipped) = classify(level);
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();

    let field = |name: &str| -> Option<u8> {
        status.lines().find_map(|line| {
            line.strip_prefix(name)
                .and_then(|rest| rest.trim_start_matches(':').trim().parse::<u8>().ok())
        })
    };

    EnforcementReport {
        level: level.as_str().to_string(),
        applied,
        skipped,
        proc_no_new_privs: field("NoNewPrivs"),
        proc_seccomp: field("Seccomp"),
    }
}
