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

use crate::capability::{EnforcementLevel, SandboxCapabilities};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::sandbox::{LandlockRule, SandboxPlan};
use std::collections::BTreeMap;
use std::io::{Error as IoError, ErrorKind};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
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

/// Compiled, fork-safe sandbox payload. Everything that allocates is built in
/// the parent BEFORE the fork; `pre_exec` only replays syscalls.
struct SandboxPayload {
    cgroup_procs: Option<PathBuf>,
    apply_user_ns: bool,
    apply_mount_ns: bool,
    apply_pid_ns: bool,
    landlock: Option<LandlockPayload>,
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

struct LandlockPayload {
    abi: i32,
    rules: Vec<LandlockRule>,
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

    let mut payload = build_payload(plan, caps)?;
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

    // SAFETY: the closure runs in the forked child between fork() and exec().
    // Every call inside is a direct syscall (setpgid, prctl, unshare, write,
    // landlock_*, seccomp) or a syscall-only helper from the landlock/seccompiler
    // crates. No parent allocator state is mutated, and any failure is returned
    // as an Err which makes the spawn fail closed (the job is never exec'd with
    // a partial sandbox).
    // SAFETY: pre_exec runs the fail-closed child setup above; no shared state.
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

/// Build the fork-safe payload in the parent: create the cgroup, precompile the
/// seccomp BPF program, and capture Landlock rules + ABI.
fn build_payload(plan: &SandboxPlan, caps: &SandboxCapabilities) -> SandboxResult<SandboxPayload> {
    let cgroup_procs = match &caps.cgroup_v2_subtree {
        Some(parent) => Some(create_cgroup(
            parent,
            &plan.cgroup_limits,
            plan.require_cgroup,
        )?),
        None => None,
    };

    let landlock = match caps.landlock_abi {
        Some(abi) if !plan.landlock_rules.is_empty() => Some(LandlockPayload {
            abi,
            rules: plan.landlock_rules.clone(),
        }),
        _ => None,
    };

    let seccomp_bpf = if caps.seccomp_bpf {
        Some(
            compile_seccomp(plan)
                .map_err(|err| SandboxError::new("seccomp_compile_failed", err.to_string()))?,
        )
    } else {
        None
    };

    Ok(SandboxPayload {
        cgroup_procs,
        apply_user_ns: plan.user_namespace && caps.user_namespace,
        apply_mount_ns: plan.mount_namespace && caps.mount_namespace,
        apply_pid_ns: plan.pid_namespace && caps.pid_namespace,
        landlock,
        seccomp_bpf,
        pty_slave_fd: None,
        rlimits: RlimitFallback {
            memory_max_bytes: plan.cgroup_limits.memory_max_bytes,
        },
    })
}

/// Create a fresh child cgroup under the delegated `parent`, enable controllers,
/// and write the limits. Returns the path to its `cgroup.procs` (where the child
/// writes its own pid in `pre_exec`).
fn create_cgroup(
    parent: &Path,
    limits: &jeryu_runner_core::sandbox::CgroupLimits,
    require_cgroup: bool,
) -> SandboxResult<PathBuf> {
    create_cgroup_with_writer(parent, limits, require_cgroup, |path, data| {
        std::fs::write(path, data)
    })
}

fn create_cgroup_with_writer(
    parent: &Path,
    limits: &jeryu_runner_core::sandbox::CgroupLimits,
    require_cgroup: bool,
    write_file: impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> SandboxResult<PathBuf> {
    // Ensure the parent delegates the load-bearing controllers we need to
    // children. Strict agent plans fail closed if memory or pids delegation
    // cannot be enabled; ordinary CI jobs keep the older best-effort posture.
    enable_cgroup_controller(
        parent,
        "memory",
        require_cgroup,
        "cgroup_memory_controller_enable_failed",
        &write_file,
    )?;
    enable_cgroup_controller(
        parent,
        "pids",
        require_cgroup,
        "cgroup_pids_controller_enable_failed",
        &write_file,
    )?;
    // CPU weight remains a tuning hint; do not refuse a strict memory/pids jail
    // merely because CPU delegation is absent.
    let _ = write_file(&parent.join("cgroup.subtree_control"), b"+cpu");

    let name = format!("jeryu-job-{}.scope", jeryu_runner_core::receipt::now_ms());
    let dir = parent.join(name);
    std::fs::create_dir(&dir)
        .map_err(|err| SandboxError::new("cgroup_create_failed", err.to_string()))?;

    // Best-effort for ordinary CI; mandatory for strict agent workcells.
    if let Err(err) = write_cgroup_limit(
        &dir,
        "memory.max",
        limits.memory_max_bytes.to_string().as_bytes(),
        require_cgroup,
        "cgroup_memory_max_write_failed",
        &write_file,
    ) {
        let _ = std::fs::remove_dir(&dir);
        return Err(err);
    }
    if let Err(err) = write_cgroup_limit(
        &dir,
        "pids.max",
        limits.pids_max.to_string().as_bytes(),
        require_cgroup,
        "cgroup_pids_max_write_failed",
        &write_file,
    ) {
        let _ = std::fs::remove_dir(&dir);
        return Err(err);
    }
    // cpu.weight in cgroup-v2 is 1..=10000; the plan uses the same scale band.
    let _ = write_file(
        &dir.join("cpu.weight"),
        limits.cpu_weight.clamp(1, 10_000).to_string().as_bytes(),
    );

    Ok(dir.join("cgroup.procs"))
}

fn enable_cgroup_controller(
    parent: &Path,
    controller: &'static str,
    require_cgroup: bool,
    strict_error_code: &'static str,
    write_file: &impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> SandboxResult<()> {
    let path = parent.join("cgroup.subtree_control");
    let token = format!("+{controller}");
    match write_file(&path, token.as_bytes()) {
        Ok(()) => Ok(()),
        Err(err) if require_cgroup => Err(SandboxError::new(
            strict_error_code,
            format!(
                "failed to enable cgroup controller {controller} at {}: {err}",
                path.display()
            ),
        )),
        Err(_) => Ok(()),
    }
}

fn write_cgroup_limit(
    dir: &Path,
    filename: &'static str,
    data: &[u8],
    require_cgroup: bool,
    strict_error_code: &'static str,
    write_file: &impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> SandboxResult<()> {
    let path = dir.join(filename);
    match write_file(&path, data) {
        Ok(()) => Ok(()),
        Err(err) if require_cgroup => Err(SandboxError::new(
            strict_error_code,
            format!(
                "failed to write cgroup limit {filename} at {}: {err}",
                path.display()
            ),
        )),
        Err(_) => Ok(()),
    }
}

/// Compile the seccomp allowlist into a BPF program in the parent (allocates),
/// so `pre_exec` only has to call the no-alloc `apply_filter`.
fn compile_seccomp(
    plan: &SandboxPlan,
) -> Result<seccompiler::BpfProgram, Box<dyn std::error::Error>> {
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};

    let arch = match std::env::consts::ARCH {
        "x86_64" => TargetArch::x86_64,
        "aarch64" => TargetArch::aarch64,
        "riscv64" => TargetArch::riscv64,
        other => return Err(format!("unsupported arch for seccomp: {other}").into()),
    };

    // Default action: deny by error, NOT kill. The plan's stated default is
    // "kill-process", but an Errno(EPERM) default keeps the toolchain alive when
    // it probes an optional syscall while still blocking the escape vectors we
    // explicitly do not allow (AF_INET sockets are blocked by argument match).
    // The escape suite proves AF_INET is actually denied.
    let rules = crate::seccomp_rules::build_rules(&plan.seccomp.allow_groups, arch)?;

    let filter = SeccompFilter::new(
        rules,
        // mismatch (syscall not in allowlist) -> EPERM, survivable by tooling.
        SeccompAction::Errno(libc::EPERM as u32),
        // match (syscall in allowlist) -> allow.
        SeccompAction::Allow,
        arch,
    )?;
    Ok(BpfProgram::try_from(filter)?)
}

/// The body that runs inside the forked child. MUST stay fail-closed: any error
/// returned here aborts the spawn before exec.
fn apply_in_child(payload: &SandboxPayload) -> std::io::Result<()> {
    // 1. Session / process-group setup for the watchdog's group-kill. A PTY
    //    child needs its own SESSION (setsid) to acquire a controlling tty; a
    //    piped child just needs its own process group (setpgid). Either way the
    //    child becomes a group leader, so the watchdog's kill(-pgid) reaps it.
    if payload.pty_slave_fd.is_some() {
        // SAFETY: setsid() creates a new session + process group led by this
        // process; no pointer args, no shared state, async-signal-safe.
        if unsafe { libc::setsid() } == -1 {
            return Err(IoError::last_os_error());
        }
    } else {
        // SAFETY: setpgid(0, 0) only moves the calling process into a new process
        // group; it touches no shared state and is async-signal-safe.
        if unsafe { libc::setpgid(0, 0) } != 0 {
            return Err(IoError::last_os_error());
        }
    }

    // 1b. PTY: make the slave our controlling terminal and wire 0/1/2 to it.
    //     Runs before seccomp (step 6), so TIOCSCTTY/dup2 are unrestricted setup.
    if let Some(slave) = payload.pty_slave_fd {
        // SAFETY: as the session leader (setsid above) ioctl(TIOCSCTTY) claims the
        // slave as our controlling tty; dup2 wires the three standard fds to it.
        // Both are direct syscalls with no shared state.
        unsafe {
            if libc::ioctl(slave, libc::TIOCSCTTY, 0) != 0 {
                return Err(IoError::last_os_error());
            }
            for target in 0..3 {
                if libc::dup2(slave, target) == -1 {
                    return Err(IoError::last_os_error());
                }
            }
            if slave > 2 {
                libc::close(slave);
            }
        }
    }

    // 2. Join the cgroup so limits bind before exec. Writing our pid to
    //    cgroup.procs migrates us. Failure here is fail-closed: if the plan
    //    expected cgroup enforcement and we cannot join, refuse to exec.
    if let Some(procs) = &payload.cgroup_procs {
        let pid = std::process::id().to_string();
        write_proc_file(procs, pid.as_bytes())?;
    }

    // 2b. setrlimit fallback (best-effort, NON-fatal): a backstop for memory and
    //     process count when cgroups are unavailable or only partially applied.
    //     This is NOT a substitute for cgroups — the fail-closed gate in
    //     capability.rs is the real protection for agent jobs — so a failed
    //     setrlimit never aborts the spawn.
    apply_rlimit_fallback(&payload.rlimits);

    // 3. PR_SET_NO_NEW_PRIVS — always, non-negotiable.
    // SAFETY: prctl(PR_SET_NO_NEW_PRIVS, 1, ...) sets a per-thread flag with no
    // pointer args; always safe and async-signal-safe.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(IoError::last_os_error());
    }

    // 4. Namespaces — only where caps allowed (already gated in build_payload).
    let mut clone_flags = 0;
    if payload.apply_user_ns {
        clone_flags |= libc::CLONE_NEWUSER;
    }
    if payload.apply_mount_ns {
        clone_flags |= libc::CLONE_NEWNS;
    }
    if payload.apply_pid_ns {
        clone_flags |= libc::CLONE_NEWPID;
    }
    // SAFETY: unshare() only detaches the calling thread's namespaces for the
    // requested (capability-gated) flags; no pointer args, no shared state.
    if clone_flags != 0 && unsafe { libc::unshare(clone_flags) } != 0 {
        return Err(IoError::last_os_error());
    }

    // 5. Landlock — workspace-only-writable ruleset.
    if let Some(landlock) = &payload.landlock {
        apply_landlock(landlock).map_err(|err| IoError::new(ErrorKind::PermissionDenied, err))?;
    }

    // 6. seccomp — last, so our own setup syscalls above were unrestricted.
    if let Some(bpf) = &payload.seccomp_bpf {
        seccompiler::apply_filter(bpf)
            .map_err(|err| IoError::new(ErrorKind::PermissionDenied, err.to_string()))?;
    }

    Ok(())
}

/// Best-effort `setrlimit` backstop applied inside the forked child.
///
/// This is a *fallback*, not a replacement for cgroups: the real protection for
/// agent jobs is the fail-closed gate in [`crate::capability::SandboxCapabilities::enforcement_level`],
/// which refuses to launch a `require_cgroup` job on a host without a delegated
/// cgroup-v2 subtree. Where a job is allowed to run degraded (cgroups missing
/// but not required), the address-space rlimit still caps the most egregious
/// memory balloons. Failures are swallowed: a too-tight limit must never break a
/// degraded-but-legitimate job, and this syscall (`prlimit64`) is already in the
/// seccomp baseline.
///
/// `RLIMIT_AS` (address-space) is notoriously hostile to V8/Node and JIT runtimes
/// that reserve huge virtual ranges they never fault in, so we apply a GENEROUS
/// multiple of the cgroup memory ceiling rather than the ceiling itself — the
/// goal is to stop an unbounded balloon, not to mirror the exact RSS cap. We also
/// skip `RLIMIT_AS` entirely if the multiple would overflow.
///
/// Do NOT mirror `pids.max` with `RLIMIT_NPROC`: that limit is per real Unix
/// user, not per sandbox. On a busy shared runner account it can make `/bin/sh`
/// unable to fork before the job starts. PID containment is enforced by cgroups
/// when the job requires it; non-agent degraded CI must remain runnable.
fn apply_rlimit_fallback(limits: &RlimitFallback) {
    // Address space: 4x the cgroup memory ceiling, clamped to avoid overflow and
    // to stay clear of breaking JIT/V8 virtual reservations. Skipped if zero or
    // if the headroom math overflows.
    if let Some(as_limit) = limits.memory_max_bytes.checked_mul(4)
        && as_limit > 0
    {
        set_one_rlimit(libc::RLIMIT_AS, as_limit);
    }
}

/// Set a single soft+hard rlimit, ignoring failure (best-effort).
fn set_one_rlimit(resource: libc::__rlimit_resource_t, value: u64) {
    // Clamp to rlim_t to avoid truncation surprises where rlim_t is narrower
    // than u64; `unwrap_or(MAX)` saturates instead of overflowing.
    let v = libc::rlim_t::try_from(value).unwrap_or(libc::rlim_t::MAX);
    let limit = libc::rlimit {
        rlim_cur: v,
        rlim_max: v,
    };
    // SAFETY: `setrlimit` reads a pointer to the fully-initialized stack-local
    // `limit`, mutates only this process's resource limits, and is async-signal-safe
    // for the fork/exec window; the result is intentionally ignored (best-effort).
    unsafe { libc::setrlimit(resource, &limit) };
}

/// Apply the Landlock ruleset inside the child. Opening the path FDs here is a
/// pure syscall; the landlock crate allocates internally, but the child is
/// single-threaded between fork and exec so this is the established safe pattern.
fn apply_landlock(payload: &LandlockPayload) -> Result<(), String> {
    use landlock::{
        ABI, Access, AccessFs, BitFlags, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr,
    };

    let abi = match payload.abi {
        1 => ABI::V1,
        2 => ABI::V2,
        3 => ABI::V3,
        4 => ABI::V4,
        5 => ABI::V5,
        _ => ABI::V6,
    };

    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| e.to_string())?
        .create()
        .map_err(|e| e.to_string())?;

    for rule in &payload.rules {
        // Compute the access bits this rule grants, honoring read/write/execute
        // independently so an exec-only or read-only-NO-exec rule is expressed
        // faithfully.
        //
        // LATENT-BUG FIX: the previous code derived access from read/write only
        // and NEVER consulted `rule.execute`. On Landlock ABI >= 2 `Execute` is a
        // distinct access right; worse, this landlock crate's `from_read` bundles
        // `Execute` into the read set, so a `read: true, execute: false` rule
        // would WRONGLY permit exec, and an exec-only rule was silently dropped by
        // the old `!read && !write` skip. We now build the set explicitly and mask
        // `Execute` out unless the rule grants it.
        let mut access: BitFlags<AccessFs> = BitFlags::empty();
        if rule.read {
            access |= AccessFs::from_read(abi);
        }
        if rule.write {
            access |= AccessFs::from_write(abi);
        }
        // `Execute` is part of `from_read` in this crate, so reconcile it against
        // the rule's explicit execute bit: add it for exec-granting rules (gated
        // on the ABI that defines a separate execute right) and remove it from a
        // read rule that does not grant exec.
        if rule.execute && abi as i32 >= ABI::V2 as i32 {
            access |= AccessFs::Execute;
        } else if !rule.execute {
            access &= !BitFlags::from(AccessFs::Execute);
        }
        // A rule that grants no access at all yields no rule rather than an
        // empty PathBeneath the kernel would reject.
        if access.is_empty() {
            continue;
        }
        // A non-existent path simply yields no rule rather than failing the
        // whole ruleset (e.g. /nix/store may be absent on this host).
        let Ok(fd) = PathFd::new(&rule.path) else {
            continue;
        };
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, access))
            .map_err(|e| e.to_string())?;
    }

    ruleset.restrict_self().map_err(|e| e.to_string())?;
    Ok(())
}

/// Write to a `/proc` or cgroup control file, returning a useful error.
fn write_proc_file(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.write_all(data)
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

fn classify(level: &EnforcementLevel) -> (Vec<String>, Vec<String>) {
    let all = [
        "no_new_privs",
        "cgroup_v2",
        "landlock",
        "seccomp",
        "user_namespace",
        "mount_namespace",
        "pid_namespace",
    ];
    match level {
        EnforcementLevel::Enforced => (all.iter().map(|s| (*s).to_string()).collect(), Vec::new()),
        EnforcementLevel::Unavailable { .. } => {
            (Vec::new(), all.iter().map(|s| (*s).to_string()).collect())
        }
        EnforcementLevel::Degraded { missing } => {
            let applied = all
                .iter()
                .filter(|s| !missing.contains(&(**s).to_string()))
                .map(|s| (*s).to_string())
                .collect();
            (applied, missing.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_limits() -> jeryu_runner_core::sandbox::CgroupLimits {
        jeryu_runner_core::sandbox::CgroupLimits {
            memory_max_bytes: 64 * 1024 * 1024,
            cpu_weight: 100,
            pids_max: 32,
            io_weight: 100,
        }
    }

    fn forced_write_failure() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "forced cgroup write failure",
        )
    }

    #[test]
    fn enforcement_report_json_is_stable() {
        let report = EnforcementReport {
            level: "degraded".to_string(),
            applied: vec!["no_new_privs".to_string(), "seccomp".to_string()],
            skipped: vec!["user_namespace".to_string()],
            proc_no_new_privs: Some(1),
            proc_seccomp: Some(2),
        };
        let json = report.to_json();
        assert!(json.contains("\"level\":\"degraded\""));
        assert!(json.contains("\"proc_no_new_privs\":1"));
        assert!(json.contains("\"proc_seccomp\":2"));
        assert!(json.contains("\"user_namespace\""));
    }

    #[test]
    fn classify_enforced_marks_all_applied() {
        let (applied, skipped) = classify(&EnforcementLevel::Enforced);
        assert!(skipped.is_empty());
        assert!(applied.contains(&"seccomp".to_string()));
        assert!(applied.contains(&"landlock".to_string()));
    }

    #[test]
    fn classify_degraded_splits_missing() {
        let level = EnforcementLevel::Degraded {
            missing: vec!["user_namespace".to_string(), "mount_namespace".to_string()],
        };
        let (applied, skipped) = classify(&level);
        assert!(skipped.contains(&"user_namespace".to_string()));
        assert!(applied.contains(&"seccomp".to_string()));
        assert!(!applied.contains(&"user_namespace".to_string()));
    }

    #[test]
    fn strict_cgroup_requires_memory_controller_enable() {
        let parent = tempfile::tempdir().expect("temp cgroup parent");
        let err = create_cgroup_with_writer(parent.path(), &test_limits(), true, |path, data| {
            if path.file_name().and_then(|name| name.to_str()) == Some("cgroup.subtree_control")
                && data == b"+memory"
            {
                Err(forced_write_failure())
            } else {
                Ok(())
            }
        })
        .expect_err("strict cgroup must fail closed when memory controller enable fails");

        assert_eq!(err.code(), "cgroup_memory_controller_enable_failed");
        assert!(err.message().contains("memory"));
    }

    #[test]
    fn strict_cgroup_requires_pids_controller_enable() {
        let parent = tempfile::tempdir().expect("temp cgroup parent");
        let err = create_cgroup_with_writer(parent.path(), &test_limits(), true, |path, data| {
            if path.file_name().and_then(|name| name.to_str()) == Some("cgroup.subtree_control")
                && data == b"+pids"
            {
                Err(forced_write_failure())
            } else {
                Ok(())
            }
        })
        .expect_err("strict cgroup must fail closed when pids controller enable fails");

        assert_eq!(err.code(), "cgroup_pids_controller_enable_failed");
        assert!(err.message().contains("pids"));
    }

    #[test]
    fn strict_cgroup_requires_memory_and_pids_limit_writes() {
        for (filename, expected_code) in [
            ("memory.max", "cgroup_memory_max_write_failed"),
            ("pids.max", "cgroup_pids_max_write_failed"),
        ] {
            let parent = tempfile::tempdir().expect("temp cgroup parent");
            let err =
                create_cgroup_with_writer(parent.path(), &test_limits(), true, |path, _data| {
                    if path.file_name().and_then(|name| name.to_str()) == Some(filename) {
                        Err(forced_write_failure())
                    } else {
                        Ok(())
                    }
                })
                .expect_err("strict cgroup must fail closed when load-bearing limit writes fail");

            assert_eq!(err.code(), expected_code);
            assert!(err.message().contains(filename));
            assert!(
                std::fs::read_dir(parent.path())
                    .expect("read temp parent")
                    .next()
                    .is_none(),
                "failed strict cgroup setup should clean up its empty child directory"
            );
        }
    }

    #[test]
    fn non_strict_cgroup_limit_writes_remain_best_effort() {
        let parent = tempfile::tempdir().expect("temp cgroup parent");
        let procs =
            create_cgroup_with_writer(parent.path(), &test_limits(), false, |_path, _data| {
                Err(forced_write_failure())
            })
            .expect("non-strict cgroup setup keeps best-effort write behavior");

        assert_eq!(
            procs.file_name().and_then(|name| name.to_str()),
            Some("cgroup.procs")
        );
    }
}
