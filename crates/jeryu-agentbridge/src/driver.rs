//! In-cell agent driver.
//!
//! This module runs a code-writing process (an LLM CLI in production; the
//! deterministic `jeryu-editbot` binary in tests) JAILED inside a cell checkout.
//! It models its launch pattern on `jeryu-runner-native`'s `NativeRunner`:
//!
//! 1. build a [`JobRequest`] confined to the cell workspace
//!    (`NetworkPolicy::Deny`, `TrustTier::T1ProtectedInternal`),
//! 2. `select_runner` -> `SandboxPlan::from_decision(&workspace, &decision)`,
//! 3. `spawn_sandboxed` (the single unsafe island applies cgroups, Landlock,
//!    seccomp, namespaces in `pre_exec`, FAIL-CLOSED),
//! 4. supervise the child under a wall-clock timeout AND a captured-output
//!    budget; either limit kills the child and is reported honestly.
//!
//! All unsafe lives in `jeryu-sandbox-linux`; this crate stays SAFE. Where the
//! native runner can call [`run_with_watchdog`] (which drains output to
//! completion), the driver instead supervises the pipes itself so it can cap
//! the captured bytes and kill the child the instant the budget is exceeded —
//! the watchdog has no notion of an output budget. The timeout half mirrors the
//! watchdog's deadline semantics exactly.

use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::{PolicyDecision, select_runner};
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::TrustTier;
use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::launch::spawn_sandboxed;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

/// What the agent should run inside the cell: a program, its arguments, and any
/// extra environment the bot needs (e.g. which file the edit-bot should write).
#[derive(Debug, Clone, Default)]
pub struct CommandSpec {
    /// Executable path. For an in-cell binary this is a path UNDER the cell
    /// workspace (see [`stage_editbot`]) so it is reachable under the
    /// workspace-only Landlock rule.
    pub program: String,
    /// Arguments passed to the program.
    pub args: Vec<String>,
    /// Extra environment entries merged on top of the sandbox base env.
    pub env: BTreeMap<String, String>,
    /// Optional stdin payload written immediately after spawn.
    pub stdin: Option<String>,
}

impl CommandSpec {
    /// Convenience constructor for a program with no args and no extra env.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            stdin: None,
        }
    }

    /// Builder: append an argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Builder: set an environment entry.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Builder: set the stdin payload written after spawn.
    #[must_use]
    pub fn stdin(mut self, text: impl Into<String>) -> Self {
        self.stdin = Some(text.into());
        self
    }
}

/// Structured progress emitted by the driver. A Phase 2 / Codex WebSocket layer
/// can subscribe through [`AgentEventSink`]; the driver itself never touches WS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    /// The sandboxed child has started; carries its host pid.
    Started {
        /// Host process id of the launched child.
        pid: u32,
    },
    /// One line captured from the child's stdout.
    Stdout(String),
    /// One line captured from the child's stderr.
    Stderr(String),
    /// Output-budget progress: bytes used so far vs. the cap.
    Budget {
        /// Captured stdout+stderr bytes so far.
        used: usize,
        /// Configured byte cap.
        limit: usize,
    },
    /// The run finished (cleanly, by timeout, or by budget kill).
    Finished {
        /// Exit code if the process exited normally.
        exit_code: Option<i32>,
        /// True when the wall-clock budget was exceeded and the child killed.
        timed_out: bool,
        /// True when the output budget was exceeded and the child killed.
        budget_exceeded: bool,
    },
}

/// Sink for [`AgentEvent`]s. The default implementation collects into a `Vec`
/// for tests; production transports (the Phase 2 WebSocket) implement this to
/// stream progress to subscribers. `emit` takes `&self` so a sink can be shared
/// behind an `Arc` across the driver's drain threads if needed.
pub trait AgentEventSink {
    /// Emit one event.
    fn emit(&self, ev: AgentEvent);
}

/// A `Vec`-collecting sink for tests and inspection. Interior-mutable so it can
/// satisfy the `&self` emit contract without forcing callers to hold `&mut`.
#[derive(Debug, Default)]
pub struct CollectingSink {
    events: std::sync::Mutex<Vec<AgentEvent>>,
}

impl CollectingSink {
    /// Create an empty collecting sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the events emitted so far.
    pub fn events(&self) -> Vec<AgentEvent> {
        self.events.lock().expect("sink mutex poisoned").clone()
    }
}

impl AgentEventSink for CollectingSink {
    fn emit(&self, ev: AgentEvent) {
        self.events.lock().expect("sink mutex poisoned").push(ev);
    }
}

/// A no-op sink for callers that do not care about progress.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl AgentEventSink for NullSink {
    fn emit(&self, _ev: AgentEvent) {}
}

/// Outcome of a driven in-cell agent run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunResult {
    /// Exit code if the child exited normally (`None` if killed by signal).
    pub exit_code: Option<i32>,
    /// True when the wall-clock timeout fired and the child was killed.
    pub timed_out: bool,
    /// True when the captured-output budget was exceeded and the child killed.
    pub budget_exceeded: bool,
    /// Captured stdout, truncated at the budget.
    pub stdout: Vec<u8>,
    /// Captured stderr, truncated at the budget.
    pub stderr: Vec<u8>,
    /// Total captured stdout+stderr bytes.
    pub captured_bytes: usize,
    /// Resolved enforcement level string for the run (e.g. `enforced`,
    /// `degraded`, `unavailable`).
    pub enforcement_level: String,
    /// Wall-clock time actually spent.
    pub elapsed: Duration,
}

impl AgentRunResult {
    /// True when the child exited cleanly (code 0, not timed out, not over budget).
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.budget_exceeded
    }
}

/// Errors raised before/while launching the in-cell agent.
#[derive(Debug)]
pub enum DriverError {
    /// The cell workspace path is unusable (missing / not a directory / I/O).
    Workspace(String),
    /// Runner policy selection failed.
    Policy(String),
    /// The kernel sandbox could not be applied (fail-closed, the bot never ran).
    SandboxUnavailable(String),
    /// Supervising the child's pipes failed (I/O on the host side).
    Supervision(String),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Workspace(m) => write!(f, "workspace: {m}"),
            Self::Policy(m) => write!(f, "policy: {m}"),
            Self::SandboxUnavailable(m) => write!(f, "sandbox_unavailable: {m}"),
            Self::Supervision(m) => write!(f, "supervision: {m}"),
        }
    }
}

impl std::error::Error for DriverError {}

/// Default output budget: 64 KiB of captured stdout+stderr.
pub const DEFAULT_OUTPUT_BUDGET_BYTES: usize = 64 * 1024;

/// In-cell agent driver. Holds the supervision budgets; the host sandbox
/// capabilities are probed once and cached process-wide.
#[derive(Debug, Clone)]
pub struct AgentDriver {
    /// Wall-clock timeout for the child.
    timeout: Duration,
    /// Cap on total captured stdout+stderr bytes. A real token budget wraps this
    /// byte budget for now.
    output_budget_bytes: usize,
    /// Require ENFORCED cgroup-v2 limits for the agent job. Defaults to `true`:
    /// an LLM-driven code generator must never run without real memory/pids
    /// confinement, so on a host lacking a delegated cgroup subtree the launch
    /// FAILS CLOSED (`DriverError::SandboxUnavailable`) instead of degrading.
    /// Callers that knowingly run on a cgroup-less host (e.g. the Landlock/seccomp
    /// jail tests) opt out via [`AgentDriver::with_require_cgroup`].
    require_cgroup: bool,
}

impl Default for AgentDriver {
    fn default() -> Self {
        Self::new(Duration::from_secs(30), DEFAULT_OUTPUT_BUDGET_BYTES)
    }
}

/// Probe the host sandbox capabilities once and cache them; the probe forks
/// throwaway children and is pointless to repeat per run.
fn cached_capabilities() -> &'static SandboxCapabilities {
    static CAPS: OnceLock<SandboxCapabilities> = OnceLock::new();
    CAPS.get_or_init(SandboxCapabilities::probe)
}

impl AgentDriver {
    /// Create a driver with an explicit timeout and output budget.
    ///
    /// Defaults `require_cgroup` to `true` (the safe agent-job posture): the run
    /// fails closed on any host without a delegated cgroup-v2 subtree.
    pub fn new(timeout: Duration, output_budget_bytes: usize) -> Self {
        Self {
            timeout,
            output_budget_bytes,
            require_cgroup: true,
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
    ///
    /// `true` (the default) fails the launch closed when the host has no
    /// delegated cgroup subtree. Pass `false` ONLY when the run is exercising the
    /// Landlock/seccomp jail rather than cgroups and must proceed on a host
    /// without cgroup delegation.
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

    /// Build the confined [`JobRequest`] for an in-cell agent run.
    ///
    /// Public so callers/tests can inspect the policy inputs the driver derives,
    /// mirroring how the native runner exposes its plan. The job is pinned to the
    /// cell workspace, network-deny, and runs at `T1ProtectedInternal` so the
    /// default native-rust-hot runner class applies the full kernel sandbox.
    pub fn build_job(&self, workspace: &Path, spec: &CommandSpec) -> JobRequest {
        JobRequest {
            job_id: format!("agent-cell-{}", jeryu_runner_core::receipt::now_ms()),
            repo_id: "in-cell-agent".to_string(),
            commit_sha: "cell".to_string(),
            workspace: workspace.to_path_buf(),
            command: spec.program.clone(),
            args: spec.args.clone(),
            env: spec.env.clone(),
            trust_tier: TrustTier::T1ProtectedInternal,
            requested_runner: None,
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::None,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: u64::try_from(self.timeout.as_millis())
                .unwrap_or(u64::MAX)
                .max(1),
            fork: false,
        }
    }

    /// Run the agent command JAILED inside `workspace`, streaming progress to
    /// `sink` and returning the supervised outcome.
    ///
    /// The pipeline mirrors the native runner: confine -> select_runner ->
    /// `SandboxPlan::from_decision` -> `spawn_sandboxed` -> supervise. The
    /// supervisor enforces both the wall-clock timeout and the captured-output
    /// budget; exceeding either kills the child and sets the matching flag.
    pub fn run<S: AgentEventSink>(
        &self,
        workspace: &Path,
        spec: &CommandSpec,
        sink: &S,
    ) -> Result<AgentRunResult, DriverError> {
        if !workspace.is_dir() {
            return Err(DriverError::Workspace(format!(
                "{} is not an existing directory",
                workspace.display()
            )));
        }

        let job = self.build_job(workspace, spec);
        let decision = select_runner(&job).map_err(|e| DriverError::Policy(e.to_string()))?;
        // Agent jobs default to require_cgroup=true: without a delegated cgroup
        // subtree this resolves to Unavailable and spawn_sandboxed refuses to
        // launch (fail-closed), surfaced below as DriverError::SandboxUnavailable.
        let plan = SandboxPlan::from_decision(workspace, &decision)
            .with_require_cgroup(self.require_cgroup);

        let caps = cached_capabilities();
        let level = caps.enforcement_level(&plan);
        let env = self.sandbox_env(&job, &decision);

        let started = Instant::now();
        // Retry on ETXTBSY ("Text file busy", os error 26): when many cells stage
        // and spawn concurrently, another thread's fork() can transiently inherit
        // the write fd to a freshly-staged binary, so exec races with the copy.
        // This is a transient race (not a sandbox failure), so retry briefly
        // rather than mis-reporting it as SandboxUnavailable.
        let mut attempt: u32 = 0;
        let mut child = loop {
            match spawn_sandboxed(&job, &plan, caps, &env) {
                Ok(child) => break child,
                Err(e)
                    if attempt < 200
                        && (e.message().contains("os error 26")
                            || e.message().contains("Text file busy")) =>
                {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(5));
                }
                // Fail-closed setup failures mean the bot never ran. Surface
                // `Unavailable` distinctly so callers can skip kernel-dependent
                // assertions honestly instead of treating it as a bot failure.
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
            }
        };

        if let Some(stdin_text) = &spec.stdin {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| DriverError::Supervision("child stdin unavailable".to_string()))?;
            stdin
                .write_all(stdin_text.as_bytes())
                .map_err(|e| DriverError::Supervision(e.to_string()))?;
        }

        sink.emit(AgentEvent::Started { pid: child.id() });

        let outcome = self.supervise(child, sink, started)?;

        sink.emit(AgentEvent::Finished {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
        });

        Ok(AgentRunResult {
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            budget_exceeded: outcome.budget_exceeded,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            captured_bytes: outcome.captured_bytes,
            enforcement_level: level.as_str().to_string(),
            elapsed: outcome.elapsed,
        })
    }

    /// Build the sandbox environment for the in-cell agent.
    ///
    /// Starts from the same hardened base the native runner uses (scrubbed,
    /// fixed PATH/HOME, secrets disabled) and then layers the bot's requested
    /// env on top so an edit-bot can be told what to write.
    fn sandbox_env(&self, job: &JobRequest, decision: &PolicyDecision) -> BTreeMap<String, String> {
        let mut env = jeryu_runner_core::fscheck::sanitize_env(&job.env);
        env.entry("HOME".to_string())
            .or_insert_with(|| "/tmp/jeryu-home".to_string());
        env.insert("TMPDIR".to_string(), "/tmp".to_string());
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert(
            "JERYU_NETWORK_POLICY".to_string(),
            decision.network_policy.as_str().to_string(),
        );
        if !decision.allow_secrets {
            env.insert("JERYU_SECRETS".to_string(), "disabled".to_string());
        }
        env
    }

    /// Supervise the live child: drain stdout/stderr on threads while polling for
    /// exit, the wall-clock deadline, and the output budget. The first of
    /// {exit, timeout, budget} wins; timeout/budget kill the child.
    fn supervise<S: AgentEventSink>(
        &self,
        mut child: Child,
        sink: &S,
        started: Instant,
    ) -> Result<SuperviseOutcome, DriverError> {
        let stdout_rx = child.stdout.take().map(spawn_line_reader);
        let stderr_rx = child.stderr.take().map(spawn_line_reader);

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut used = 0usize;
        let mut timed_out = false;
        let mut budget_exceeded = false;
        let mut stdout_done = stdout_rx.is_none();
        let mut stderr_done = stderr_rx.is_none();

        // Drain whatever lines are ready RIGHT NOW from one stream into its
        // buffer, emit per-line events, and grow the running byte total. Returns
        // true when the byte budget was tripped on this drain.
        let drain = |rx: &Option<Receiver<Line>>,
                     buf: &mut Vec<u8>,
                     used: &mut usize,
                     is_stdout: bool,
                     done: &mut bool|
         -> bool {
            if *done {
                return false;
            }
            let Some(rx) = rx else {
                *done = true;
                return false;
            };
            loop {
                match rx.try_recv() {
                    Ok(Line::Bytes(line)) => {
                        *used += line.len();
                        buf.extend_from_slice(&line);
                        let text = String::from_utf8_lossy(&line).trim_end().to_string();
                        if is_stdout {
                            sink.emit(AgentEvent::Stdout(text));
                        } else {
                            sink.emit(AgentEvent::Stderr(text));
                        }
                        sink.emit(AgentEvent::Budget {
                            used: *used,
                            limit: self.output_budget_bytes,
                        });
                        if *used > self.output_budget_bytes {
                            return true;
                        }
                    }
                    Ok(Line::Eof) | Err(TryRecvError::Disconnected) => {
                        *done = true;
                        return false;
                    }
                    Err(TryRecvError::Empty) => return false,
                }
            }
        };

        let exit_code = loop {
            // Pull any pending output first so a budget breach is seen promptly.
            if drain(&stdout_rx, &mut stdout, &mut used, true, &mut stdout_done)
                || drain(&stderr_rx, &mut stderr, &mut used, false, &mut stderr_done)
            {
                budget_exceeded = true;
                let _ = child.kill();
                let status = child
                    .wait()
                    .map_err(|e| DriverError::Supervision(e.to_string()))?;
                break status.code();
            }

            match child
                .try_wait()
                .map_err(|e| DriverError::Supervision(e.to_string()))?
            {
                Some(status) => break status.code(),
                None => {
                    if started.elapsed() >= self.timeout {
                        timed_out = true;
                        let _ = child.kill();
                        let status = child
                            .wait()
                            .map_err(|e| DriverError::Supervision(e.to_string()))?;
                        break status.code();
                    }
                    thread::sleep(poll_interval(started.elapsed(), self.timeout));
                }
            }
        };

        // Final drain of any output produced between the last poll and exit. We
        // ignore a late budget trip here (the child has already exited), but
        // wait briefly for pipe-reader EOF so fast children cannot lose their
        // last line to a scheduler race.
        let final_drain_deadline = Instant::now() + Duration::from_millis(100);
        while !(stdout_done && stderr_done) && Instant::now() < final_drain_deadline {
            let before = used;
            drain(&stdout_rx, &mut stdout, &mut used, true, &mut stdout_done);
            drain(&stderr_rx, &mut stderr, &mut used, false, &mut stderr_done);
            if used == before {
                thread::sleep(Duration::from_millis(1));
            }
        }

        // Truncate the captured buffers to the budget so a final burst cannot
        // blow the cap retroactively.
        truncate_to(&mut stdout, &mut stderr, self.output_budget_bytes);

        Ok(SuperviseOutcome {
            exit_code,
            timed_out,
            budget_exceeded,
            stdout,
            stderr,
            captured_bytes: used,
            elapsed: started.elapsed(),
        })
    }
}

struct SuperviseOutcome {
    exit_code: Option<i32>,
    timed_out: bool,
    budget_exceeded: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    captured_bytes: usize,
    elapsed: Duration,
}

/// A unit of output handed from a drain thread to the supervisor.
enum Line {
    /// One newline-terminated (or final) chunk of bytes.
    Bytes(Vec<u8>),
    /// The stream reached EOF.
    Eof,
}

/// Read a child pipe line-by-line on a dedicated thread, forwarding each line to
/// the supervisor over a channel so a chatty bot cannot deadlock against a full
/// pipe while we poll for the deadline.
fn spawn_line_reader<R: Read + Send + 'static>(reader: R) -> Receiver<Line> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let mut reader = std::io::BufReader::new(reader);
        loop {
            let mut buf = Vec::new();
            match read_line_bytes(&mut reader, &mut buf) {
                Ok(0) => {
                    let _ = tx.send(Line::Eof);
                    break;
                }
                Ok(_) => {
                    if tx.send(Line::Bytes(buf)).is_err() {
                        break; // supervisor dropped the receiver (we were killed)
                    }
                }
                Err(_) => {
                    let _ = tx.send(Line::Eof);
                    break;
                }
            }
        }
    });
    rx
}

/// Read up to and including the next `\n` (or EOF) into `buf`, returning bytes
/// read. A line longer than the budget is still chunked at 8 KiB so a single
/// unbounded line cannot defeat the per-line budget polling.
fn read_line_bytes<R: std::io::BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> std::io::Result<usize> {
    const CHUNK_CAP: usize = 8 * 1024;
    let mut read = 0usize;
    loop {
        let mut byte = [0u8; 1];
        match reader.read(&mut byte)? {
            0 => break,
            _ => {
                buf.push(byte[0]);
                read += 1;
                if byte[0] == b'\n' || read >= CHUNK_CAP {
                    break;
                }
            }
        }
    }
    Ok(read)
}

/// Truncate the captured stdout/stderr so their combined length never exceeds
/// `budget`. stdout is trimmed first, then stderr, deterministically.
fn truncate_to(stdout: &mut Vec<u8>, stderr: &mut Vec<u8>, budget: usize) {
    if stdout.len() + stderr.len() <= budget {
        return;
    }
    if stdout.len() > budget {
        stdout.truncate(budget);
    }
    let remaining = budget.saturating_sub(stdout.len());
    if stderr.len() > remaining {
        stderr.truncate(remaining);
    }
}

/// Adapt poll cadence to the deadline: tight near the end, relaxed early, so a
/// long agent run is cheap to supervise but a timeout fires promptly. Mirrors
/// the watchdog's cadence.
fn poll_interval(elapsed: Duration, timeout: Duration) -> Duration {
    let remaining = timeout.saturating_sub(elapsed);
    remaining
        .min(Duration::from_millis(10))
        .max(Duration::from_millis(1))
}

/// Stage the deterministic `jeryu-editbot` binary INTO the cell `workspace` so
/// it is reachable under the workspace-only Landlock rule, mirroring the
/// self-copy-into-checkout trick the sandbox launch path relies on. Returns the
/// program path (relative to the workspace cwd) to put in a [`CommandSpec`].
///
/// `editbot_src` is the host path of the compiled edit-bot (in tests, the
/// `CARGO_BIN_EXE_jeryu-editbot` env var). The staged copy is named
/// `.jeryu-editbot` and made executable.
pub fn stage_editbot(workspace: &Path, editbot_src: &Path) -> Result<PathBuf, DriverError> {
    let staged = workspace.join(".jeryu-editbot");
    std::fs::copy(editbot_src, &staged).map_err(|e| {
        DriverError::Workspace(format!(
            "stage editbot {} -> {}: {e}",
            editbot_src.display(),
            staged.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&staged)
            .map_err(|e| DriverError::Workspace(e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&staged, perms)
            .map_err(|e| DriverError::Workspace(e.to_string()))?;
    }
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collecting_sink_records_events_in_order() {
        let sink = CollectingSink::new();
        sink.emit(AgentEvent::Started { pid: 7 });
        sink.emit(AgentEvent::Stdout("hi".to_string()));
        sink.emit(AgentEvent::Finished {
            exit_code: Some(0),
            timed_out: false,
            budget_exceeded: false,
        });
        let events = sink.events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], AgentEvent::Started { pid: 7 });
        assert!(matches!(events[1], AgentEvent::Stdout(_)));
        assert!(matches!(events[2], AgentEvent::Finished { .. }));
    }

    #[test]
    fn driver_defaults_to_require_cgroup_and_builder_opts_out() {
        // Agent jobs are confined by default: require_cgroup is ON.
        assert!(AgentDriver::default().require_cgroup());
        assert!(AgentDriver::new(Duration::from_secs(1), 1024).require_cgroup());
        // Tests that exercise only the Landlock/seccomp jail opt out.
        assert!(
            !AgentDriver::default()
                .with_require_cgroup(false)
                .require_cgroup()
        );
    }

    #[test]
    fn build_job_confines_to_workspace_with_deny_network() {
        let driver = AgentDriver::default();
        let ws = PathBuf::from("/tmp/jeryu-cell-xyz");
        let job = driver.build_job(&ws, &CommandSpec::new("/bin/true"));
        assert_eq!(job.workspace, ws);
        assert_eq!(job.network_policy, NetworkPolicy::Deny);
        assert_eq!(job.trust_tier, TrustTier::T1ProtectedInternal);
        // T1 default selects native-rust-hot, the full kernel sandbox class.
        let decision = select_runner(&job).expect("policy");
        assert!(decision.runner_class.is_native());
        assert_eq!(decision.network_policy, NetworkPolicy::Deny);
    }

    #[test]
    fn truncate_to_caps_combined_length() {
        let mut out = vec![b'a'; 100];
        let mut err = vec![b'b'; 100];
        truncate_to(&mut out, &mut err, 150);
        assert_eq!(out.len() + err.len(), 150);
        assert_eq!(out.len(), 100);
        assert_eq!(err.len(), 50);
    }

    #[test]
    fn truncate_to_is_noop_under_budget() {
        let mut out = vec![b'a'; 10];
        let mut err = vec![b'b'; 10];
        truncate_to(&mut out, &mut err, 100);
        assert_eq!(out.len(), 10);
        assert_eq!(err.len(), 10);
    }

    #[test]
    fn run_rejects_missing_workspace() {
        let driver = AgentDriver::default();
        let err = driver
            .run(
                Path::new("/nonexistent/jeryu/cell"),
                &CommandSpec::new("/bin/true"),
                &NullSink,
            )
            .expect_err("missing workspace must error");
        assert!(matches!(err, DriverError::Workspace(_)));
    }
}
