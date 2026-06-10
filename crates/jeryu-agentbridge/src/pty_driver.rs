//! PTY-backed in-cell agent driver.
//!
//! Where [`crate::driver::AgentDriver`] supervises a jailed child over pipes,
//! [`PtyAgentDriver`] gives the child a real controlling terminal (via
//! [`jeryu_sandbox_linux::ChildIo::Pty`] + the `pty` seccomp group) so coding-
//! agent CLIs that expect a TTY run correctly. It streams the merged terminal
//! output to an [`AgentEventSink`] and applies inbound [`AgentControl`] commands
//! (operator / JPMC steering) to the live agent. The kernel confinement is
//! identical to the piped driver; ALL `unsafe` stays in `jeryu-sandbox-linux`
//! (this crate is SAFE), so the few syscall-level control ops go through that
//! crate's safe-API helpers ([`signal_group`], [`resize_pty`]).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;
use std::process::Child;
use std::sync::OnceLock;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::{PolicyDecision, select_runner};
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::TrustTier;
use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::{
    ChildIo, GroupSignal, open_pty, resize_pty, signal_group, spawn_command_on_pty,
    spawn_sandboxed_with_io,
};

use crate::driver::{
    AgentEvent, AgentEventSink, AgentRunResult, CommandSpec, DEFAULT_OUTPUT_BUDGET_BYTES,
    DriverError,
};

/// A steering command applied to a live PTY agent. Mirrors the transport-level
/// control schema (`jeryu_agent_stream::AgentControlCommand`) 1:1 so the runtime
/// maps between them without this SAFE crate depending on the stream crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentControl {
    /// Raw bytes written to the agent's stdin (the PTY master).
    SendInput(Vec<u8>),
    /// Write `text` followed by a newline to the agent's stdin.
    InjectPrompt(String),
    /// Send SIGINT (Ctrl-C) to the agent's process group.
    Interrupt,
    /// Terminate the agent: SIGTERM, then SIGKILL after a short grace period.
    Terminate,
    /// Resize the agent's terminal window.
    ResizePty {
        /// Rows.
        rows: u16,
        /// Columns.
        cols: u16,
    },
    /// Raise the captured-output byte budget by `bytes` (extend a long run).
    RaiseBudget(usize),
}

/// A non-blocking source of [`AgentControl`] commands polled each supervise tick.
pub trait AgentControlSource {
    /// Return the next pending control command, or `None` if none is ready.
    fn try_recv(&self) -> Option<AgentControl>;
}

/// A control source that never produces a command (fire-and-forget runs).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoControl;

impl AgentControlSource for NoControl {
    fn try_recv(&self) -> Option<AgentControl> {
        None
    }
}

/// An `mpsc::Receiver<AgentControl>` is a control source: the orchestrator holds
/// the `Sender` and forwards mapped transport commands to the driver.
impl AgentControlSource for Receiver<AgentControl> {
    fn try_recv(&self) -> Option<AgentControl> {
        Receiver::try_recv(self).ok()
    }
}

/// Probe the host sandbox capabilities once and cache them process-wide.
fn cached_capabilities() -> &'static SandboxCapabilities {
    static CAPS: OnceLock<SandboxCapabilities> = OnceLock::new();
    CAPS.get_or_init(SandboxCapabilities::probe)
}

/// PTY-backed in-cell agent driver. Holds the supervision budgets; the kernel
/// confinement and `unsafe` syscalls live in `jeryu-sandbox-linux`.
#[derive(Debug, Clone)]
pub struct PtyAgentDriver {
    timeout: Duration,
    output_budget_bytes: usize,
    require_cgroup: bool,
    grace: Duration,
}

impl Default for PtyAgentDriver {
    fn default() -> Self {
        Self::new(Duration::from_secs(30), DEFAULT_OUTPUT_BUDGET_BYTES)
    }
}

impl PtyAgentDriver {
    /// Create a driver with an explicit timeout and output budget (fail-closed
    /// cgroup posture by default, like [`crate::driver::AgentDriver`]).
    pub fn new(timeout: Duration, output_budget_bytes: usize) -> Self {
        Self {
            timeout,
            output_budget_bytes,
            require_cgroup: true,
            grace: Duration::from_millis(200),
        }
    }

    /// Builder: set the wall-clock timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builder: set the captured-output byte budget.
    #[must_use]
    pub fn with_output_budget(mut self, bytes: usize) -> Self {
        self.output_budget_bytes = bytes;
        self
    }

    /// Builder: require (or not) enforced cgroup-v2 limits for the agent job.
    #[must_use]
    pub fn with_require_cgroup(mut self, require: bool) -> Self {
        self.require_cgroup = require;
        self
    }

    /// Whether this driver requires enforced cgroup-v2 limits.
    #[must_use]
    pub fn require_cgroup(&self) -> bool {
        self.require_cgroup
    }

    /// Build the confined job for a PTY agent run. Network-deny by default; the
    /// egress posture is layered on by the runtime (R9 `EgressProxyOnly`).
    fn build_job(&self, workspace: &Path, spec: &CommandSpec) -> JobRequest {
        JobRequest {
            job_id: format!("pty-agent-{}", jeryu_runner_core::receipt::now_ms()),
            repo_id: "in-cell-agent".to_string(),
            commit_sha: "cell".to_string(),
            workspace: workspace.to_path_buf(),
            command: spec.program.clone(),
            args: spec.args.clone(),
            env: spec.env.clone(),
            trust_tier: TrustTier::T1ProtectedInternal,
            requested_runner: None,
            network_policy: NetworkPolicy::EgressOnly,
            secret_policy: SecretPolicy::None,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: u64::try_from(self.timeout.as_millis())
                .unwrap_or(u64::MAX)
                .max(1),
            fork: false,
        }
    }

    /// Hardened base env + a `TERM` so the agent CLI renders for the PTY.
    fn sandbox_env(&self, job: &JobRequest, decision: &PolicyDecision) -> BTreeMap<String, String> {
        let mut env = jeryu_runner_core::fscheck::sanitize_env(&job.env);
        // Point HOME at the workspace agent-home where seeded auth lives.
        let agent_home = job.workspace.join(".agent-home");
        let _ = std::fs::create_dir_all(&agent_home);
        env.insert("HOME".to_string(), agent_home.display().to_string());
        env.insert("TMPDIR".to_string(), "/tmp".to_string());
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        // Force Go's net package to use its pure-Go DNS resolver which
        // reads /etc/resolv.conf (the CGO resolver fails in static binaries).
        // We point GODEBUG=netdns=go and create a custom resolv.conf with
        // Google DNS (8.8.8.8) so Go doesn't try [::1]:53.
        env.insert("GODEBUG".to_string(), "netdns=go".to_string());
        // Suppress tcmalloc warnings from Go/C++ agent binaries.
        env.insert("GOMEMLIMIT".to_string(), "4GiB".to_string());
        env.insert(
            "TCMALLOC_LARGE_ALLOC_REPORT_THRESHOLD".to_string(),
            "10737418240".to_string(),
        );
        // Set initial window size hints.
        env.insert("COLUMNS".to_string(), "120".to_string());
        env.insert("LINES".to_string(), "40".to_string());
        env.insert(
            "JERYU_NETWORK_POLICY".to_string(),
            decision.network_policy.as_str().to_string(),
        );
        if !decision.allow_secrets {
            env.insert("JERYU_SECRETS".to_string(), "disabled".to_string());
        }
        env
    }

    /// Build the hardened PTY sandbox plan and the job/decision pair it depends
    /// on. Tests call this directly so they can assert the exact hardening
    /// posture without spawning a child process.
    fn build_sandbox_plan(
        &self,
        workspace: &Path,
        spec: &CommandSpec,
    ) -> Result<(JobRequest, SandboxPlan, PolicyDecision), DriverError> {
        if !workspace.is_dir() {
            return Err(DriverError::Workspace(format!(
                "{} is not an existing directory",
                workspace.display()
            )));
        }

        let job = self.build_job(workspace, spec);
        let decision = select_runner(&job).map_err(|e| DriverError::Policy(e.to_string()))?;
        let mut plan = SandboxPlan::from_decision(workspace, &decision)
            .with_require_cgroup(self.require_cgroup);
        // Disable user namespace for PTY sessions: CLONE_NEWUSER breaks Go's
        // pure DNS resolver (falls back to [::1]:53 instead of reading
        // /etc/resolv.conf). Landlock + seccomp still provide isolation.
        plan.user_namespace = false;
        // Request the pty seccomp group: setsid + ioctl-except-TIOCSTI.
        if !plan.seccomp.allow_groups.iter().any(|g| g == "pty") {
            plan.seccomp.allow_groups.push("pty".to_string());
        }
        // PTY agents (codex, claude, agy) open /dev/tty for their TUI. Add a
        // read-write landlock rule so the kernel allows the open().
        plan.landlock_rules
            .push(jeryu_runner_core::sandbox::LandlockRule {
                path: std::path::PathBuf::from("/dev/tty"),
                read: true,
                write: true,
                execute: false,
            });
        // Also allow reading /dev/urandom and /dev/random (crypto/TLS).
        plan.landlock_rules
            .push(jeryu_runner_core::sandbox::LandlockRule {
                path: std::path::PathBuf::from("/dev/urandom"),
                read: true,
                write: false,
                execute: false,
            });
        // Allow reading /etc (SSL certs, resolv.conf, passwd, etc.).
        plan.landlock_rules
            .push(jeryu_runner_core::sandbox::LandlockRule {
                path: std::path::PathBuf::from("/etc"),
                read: true,
                write: false,
                execute: false,
            });
        // /etc/resolv.conf symlinks to /run/systemd/resolve/stub-resolv.conf;
        // landlock must allow reading the target for DNS to work.
        plan.landlock_rules
            .push(jeryu_runner_core::sandbox::LandlockRule {
                path: std::path::PathBuf::from("/run"),
                read: true,
                write: false,
                execute: false,
            });
        // Allow read-write to agent home (auth tokens, logs, cache).
        let ah = job.workspace.join(".agent-home");
        plan.landlock_rules
            .push(jeryu_runner_core::sandbox::LandlockRule {
                path: ah,
                read: true,
                write: true,
                execute: false,
            });
        // Allow read of /tmp (for TMPDIR).
        plan.landlock_rules
            .push(jeryu_runner_core::sandbox::LandlockRule {
                path: std::path::PathBuf::from("/tmp"),
                read: true,
                write: true,
                execute: true,
            });
        // Allow /lib (shared libs used by dynamically linked Go binaries).
        plan.landlock_rules
            .push(jeryu_runner_core::sandbox::LandlockRule {
                path: std::path::PathBuf::from("/lib"),
                read: true,
                write: false,
                execute: true,
            });

        // If the job allows egress, unlock AF_INET/AF_INET6 sockets in seccomp.
        if job.network_policy == NetworkPolicy::EgressOnly
            && !plan
                .seccomp
                .allow_groups
                .iter()
                .any(|g| g == "network-egress")
        {
            plan.seccomp.allow_groups.push("network-egress".to_string());
        }

        Ok((job, plan, decision))
    }

    /// Run the agent command jailed inside `workspace` with a controlling PTY,
    /// streaming terminal output to `sink` and applying `control` commands live.
    pub fn run<S: AgentEventSink, C: AgentControlSource>(
        &self,
        workspace: &Path,
        spec: &CommandSpec,
        sink: &S,
        control: &C,
    ) -> Result<AgentRunResult, DriverError> {
        let (job, plan, decision) = self.build_sandbox_plan(workspace, spec)?;

        let caps = cached_capabilities();
        let level = caps.enforcement_level(&plan);
        let env = self.sandbox_env(&job, &decision);

        let (master, slave) =
            open_pty().map_err(|e| DriverError::SandboxUnavailable(e.message().to_string()))?;
        // Set a reasonable initial window size so TUI agents (bubbletea, etc.)
        // render immediately instead of seeing a 0×0 terminal.
        let _ = resize_pty(master.as_raw_fd(), 40, 120);

        let started = Instant::now();
        let child = match spawn_sandboxed_with_io(
            &job,
            &plan,
            caps,
            &env,
            ChildIo::Pty {
                slave_fd: slave.as_raw_fd(),
            },
        ) {
            Ok(child) => child,
            Err(e) if e.code() == "sandbox_unavailable" => {
                return Err(DriverError::SandboxUnavailable(e.message().to_string()));
            }
            Err(e) => {
                return Err(DriverError::SandboxUnavailable(format!(
                    "[{}] {}",
                    e.code(),
                    e.message()
                )));
            }
        };
        // Parent closes its slave copy so the master observes EOF on child exit.
        drop(slave);

        sink.emit(AgentEvent::Started { pid: child.id() });
        let outcome = self.supervise(child, master, sink, control, started)?;
        sink.emit(AgentEvent::Finished {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
        });

        Ok(AgentRunResult {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
            stdout: outcome.captured,
            stderr: Vec::new(),
            captured_bytes: outcome.used,
            enforcement_level: level.as_str().to_string(),
            elapsed: outcome.elapsed,
        })
    }

    /// Run an arbitrary HOST command (e.g. `docker run ...`) on a controlling PTY,
    /// streaming its terminal output to `sink` and applying `control` live, exactly
    /// like [`PtyAgentDriver::run`] — but WITHOUT the kernel sandbox. This is the
    /// docker-backed agent runtime: the container engine confines the agent, so the
    /// `docker run` process itself is an ordinary child. The output pump, budget,
    /// timeout, and control handling are identical (the same [`Self::supervise`]
    /// loop drives both paths), so the only difference from `run` is the child: a
    /// plain host process instead of a sandboxed one. `cwd` is where the host
    /// command runs; the container's `-w` governs the agent's working directory.
    pub fn run_host_pty<S: AgentEventSink, C: AgentControlSource>(
        &self,
        cwd: &Path,
        spec: &CommandSpec,
        sink: &S,
        control: &C,
    ) -> Result<AgentRunResult, DriverError> {
        if !cwd.is_dir() {
            return Err(DriverError::Workspace(format!(
                "{} is not an existing directory",
                cwd.display()
            )));
        }

        let (master, slave) =
            open_pty().map_err(|e| DriverError::SandboxUnavailable(e.message().to_string()))?;
        // Set a reasonable initial window size so TUI agents (bubbletea, etc.)
        // render immediately instead of seeing a 0×0 terminal.
        let _ = resize_pty(master.as_raw_fd(), 40, 120);

        let started = Instant::now();
        let child = spawn_command_on_pty(&spec.program, &spec.args, &spec.env, cwd, &slave)
            .map_err(|e| {
                DriverError::SandboxUnavailable(format!("[{}] {}", e.code(), e.message()))
            })?;
        // Parent closes its slave copy so the master observes EOF on child exit.
        drop(slave);

        sink.emit(AgentEvent::Started { pid: child.id() });
        let outcome = self.supervise(child, master, sink, control, started)?;
        sink.emit(AgentEvent::Finished {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
        });

        Ok(AgentRunResult {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
            stdout: outcome.captured,
            stderr: Vec::new(),
            captured_bytes: outcome.used,
            // The host docker process is unsandboxed-by-design (the container is the
            // jail), so it carries no kernel-enforcement level of its own.
            enforcement_level: "container".to_string(),
            elapsed: outcome.elapsed,
        })
    }

    fn supervise<S, C>(
        &self,
        mut child: Child,
        master: OwnedFd,
        sink: &S,
        control: &C,
        started: Instant,
    ) -> Result<PtyOutcome, DriverError>
    where
        S: AgentEventSink,
        C: AgentControlSource,
    {
        let pid = child.id();
        // A reader thread blocking-reads a clone of the master so a chatty agent
        // cannot deadlock the supervisor; the supervisor keeps the master to
        // WRITE control input. Both fds refer to the same PTY.
        let mut writer = std::fs::File::from(
            master
                .try_clone()
                .map_err(|e| DriverError::Supervision(e.to_string()))?,
        );
        let mut reader = std::fs::File::from(master);
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    // EIO once every slave fd closes (the child exited).
                    Err(_) => break,
                }
            }
        });

        let mut captured = Vec::new();
        let mut used = 0usize;
        let mut budget = self.output_budget_bytes;
        let mut timed_out = false;
        let mut budget_exceeded = false;
        let mut terminate_at: Option<Instant> = None;

        let exit_code = loop {
            // 1. apply pending control commands.
            while let Some(cmd) = control.try_recv() {
                match cmd {
                    AgentControl::SendInput(bytes) => {
                        let _ = writer.write_all(&bytes);
                        let _ = writer.flush();
                    }
                    AgentControl::InjectPrompt(text) => {
                        let _ = writer.write_all(text.as_bytes());
                        let _ = writer.write_all(b"\n");
                        let _ = writer.flush();
                    }
                    AgentControl::Interrupt => signal_group(pid, GroupSignal::Interrupt),
                    AgentControl::Terminate => {
                        signal_group(pid, GroupSignal::Terminate);
                        terminate_at.get_or_insert_with(Instant::now);
                    }
                    AgentControl::ResizePty { rows, cols } => {
                        let _ = resize_pty(writer.as_raw_fd(), rows, cols);
                    }
                    AgentControl::RaiseBudget(n) => budget = budget.saturating_add(n),
                }
            }

            // 2. drain whatever the reader thread has produced.
            let mut over_budget = false;
            while let Ok(chunk) = rx.try_recv() {
                used += chunk.len();
                captured.extend_from_slice(&chunk);
                sink.emit(AgentEvent::Stdout(
                    String::from_utf8_lossy(&chunk).into_owned(),
                ));
                sink.emit(AgentEvent::Budget {
                    used,
                    limit: budget,
                });
                if used > budget {
                    over_budget = true;
                    break;
                }
            }
            if over_budget {
                budget_exceeded = true;
                signal_group(pid, GroupSignal::Kill);
                break child
                    .wait()
                    .map_err(|e| DriverError::Supervision(e.to_string()))?
                    .code();
            }

            // 3. escalate a requested terminate to SIGKILL after the grace window.
            if let Some(t) = terminate_at
                && t.elapsed() >= self.grace
            {
                signal_group(pid, GroupSignal::Kill);
            }

            // 4. exited? timed out?
            match child
                .try_wait()
                .map_err(|e| DriverError::Supervision(e.to_string()))?
            {
                Some(status) => break status.code(),
                None => {
                    if started.elapsed() >= self.timeout {
                        timed_out = true;
                        signal_group(pid, GroupSignal::Kill);
                        break child
                            .wait()
                            .map_err(|e| DriverError::Supervision(e.to_string()))?
                            .code();
                    }
                    thread::sleep(poll_interval(started.elapsed(), self.timeout));
                }
            }
        };

        // Final drain of anything produced between the last poll and exit.
        while let Ok(chunk) = rx.try_recv() {
            used += chunk.len();
            captured.extend_from_slice(&chunk);
            sink.emit(AgentEvent::Stdout(
                String::from_utf8_lossy(&chunk).into_owned(),
            ));
            sink.emit(AgentEvent::Budget {
                used,
                limit: budget,
            });
        }
        let _ = reader_handle.join();
        // The reader can win the race after the pre-join drain on fast exits.
        // Drain again after join so captured stdout and emitted events agree.
        while let Ok(chunk) = rx.try_recv() {
            used += chunk.len();
            captured.extend_from_slice(&chunk);
            sink.emit(AgentEvent::Stdout(
                String::from_utf8_lossy(&chunk).into_owned(),
            ));
            sink.emit(AgentEvent::Budget {
                used,
                limit: budget,
            });
        }
        if captured.len() > budget {
            captured.truncate(budget);
        }

        Ok(PtyOutcome {
            exit_code,
            timed_out,
            budget_exceeded,
            captured,
            used,
            elapsed: started.elapsed(),
        })
    }
}

struct PtyOutcome {
    exit_code: Option<i32>,
    timed_out: bool,
    budget_exceeded: bool,
    captured: Vec<u8>,
    used: usize,
    elapsed: Duration,
}

/// Adapt poll cadence to the deadline: tight near the end, relaxed early.
fn poll_interval(elapsed: Duration, timeout: Duration) -> Duration {
    timeout
        .saturating_sub(elapsed)
        .min(Duration::from_millis(10))
        .max(Duration::from_millis(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn sandbox_env_sets_agent_home_and_go_dns() {
        let workspace = tempfile::tempdir().expect("workspace");
        let driver = PtyAgentDriver::new(Duration::from_secs(5), 1024);
        let spec = CommandSpec::new("/bin/true");
        let (job, plan, decision) = driver
            .build_sandbox_plan(workspace.path(), &spec)
            .expect("sandbox plan");

        let env = driver.sandbox_env(&job, &decision);
        let agent_home = workspace.path().join(".agent-home");
        let expected_home = agent_home.to_string_lossy().to_string();

        assert_eq!(
            env.get("HOME").map(String::as_str),
            Some(expected_home.as_str())
        );
        assert_eq!(env.get("TMPDIR").map(String::as_str), Some("/tmp"));
        assert_eq!(env.get("GODEBUG").map(String::as_str), Some("netdns=go"));
        assert!(agent_home.is_dir(), "agent home must be created");
        assert!(
            !plan.user_namespace,
            "PTY sessions must disable user namespaces"
        );
        assert!(
            plan.landlock_rules
                .iter()
                .any(|rule| rule.path == Path::new("/run")),
            "PTY sessions must allow reading /run for DNS resolution"
        );
    }
}
