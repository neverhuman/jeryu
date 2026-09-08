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
use std::io::Read;
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
}

impl CommandSpec {
    /// Convenience constructor for a program with no args and no extra env.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
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

mod implementation;

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
