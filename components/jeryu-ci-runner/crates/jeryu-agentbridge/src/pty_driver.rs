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

const CLAUDE_THEME_PROMPT_BUFFER_LIMIT: usize = 16 * 1024;

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

mod implementation;

fn record_pty_chunk<S: AgentEventSink>(
    chunk: Vec<u8>,
    captured: &mut Vec<u8>,
    used: &mut usize,
    sink: &S,
    budget: usize,
) {
    *used += chunk.len();
    captured.extend_from_slice(&chunk);
    sink.emit(AgentEvent::Stdout(
        String::from_utf8_lossy(&chunk).into_owned(),
    ));
    sink.emit(AgentEvent::Budget {
        used: *used,
        limit: budget,
    });
}

#[derive(Debug, Default)]
struct PtyOutputFilter {
    claude_theme: ClaudeThemePromptAutoSelect,
}

impl PtyOutputFilter {
    fn filter<W: Write>(&mut self, chunk: Vec<u8>, writer: &mut W) -> Option<Vec<u8>> {
        self.claude_theme.filter(chunk, writer)
    }

    fn take_pending(&mut self) -> Option<Vec<u8>> {
        self.claude_theme.take_pending()
    }
}

#[derive(Debug, Default)]
struct ClaudeThemePromptAutoSelect {
    pending: Vec<u8>,
    selected: bool,
}

impl ClaudeThemePromptAutoSelect {
    fn filter<W: Write>(&mut self, chunk: Vec<u8>, writer: &mut W) -> Option<Vec<u8>> {
        if self.selected {
            return should_forward_after_claude_theme(&chunk).then_some(chunk);
        }

        let text = String::from_utf8_lossy(&chunk);
        if self.pending.is_empty() && !might_be_claude_theme_prompt(&text) {
            return Some(chunk);
        }

        self.pending.extend_from_slice(&chunk);
        let pending_text = String::from_utf8_lossy(&self.pending);
        if is_claude_theme_prompt(&pending_text) {
            self.selected = true;
            self.pending.clear();
            let _ = writer.write_all(b"2\r");
            let _ = writer.flush();
            return None;
        }

        if self.pending.len() > CLAUDE_THEME_PROMPT_BUFFER_LIMIT
            || !might_be_claude_theme_prompt(&pending_text)
        {
            return Some(std::mem::take(&mut self.pending));
        }

        None
    }

    fn take_pending(&mut self) -> Option<Vec<u8>> {
        if self.pending.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.pending))
        }
    }
}

fn should_forward_after_claude_theme(chunk: &[u8]) -> bool {
    !might_be_claude_theme_prompt(&String::from_utf8_lossy(chunk))
}

fn is_claude_theme_prompt(text: &str) -> bool {
    text.contains("Choose the text style that looks best with your terminal")
        && (text.contains("Syntax theme:")
            || text.contains("Dark mode")
            || text.contains("Monokai Extended"))
}

fn might_be_claude_theme_prompt(text: &str) -> bool {
    text.contains("Let's get started")
        || text.contains("Choose the text style")
        || text.contains("Syntax theme:")
        || text.contains("Monokai Extended")
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

    #[test]
    fn claude_theme_prompt_is_selected_and_suppressed() {
        let prompt = "Let's get started.\r\n\r\n Choose the text style that looks best with your terminal\r\n   1. Auto\r\n > 2. Dark mode\r\n  Syntax theme: Monokai Extended";
        let mut filter = ClaudeThemePromptAutoSelect::default();
        let mut input = Vec::new();

        let output = filter.filter(prompt.as_bytes().to_vec(), &mut input);

        assert!(output.is_none(), "theme picker must not reach the UI");
        assert_eq!(input, b"2\r");
        assert!(filter.take_pending().is_none());
    }

    #[test]
    fn claude_theme_prompt_can_span_chunks() {
        let mut filter = ClaudeThemePromptAutoSelect::default();
        let mut input = Vec::new();

        assert!(
            filter
                .filter(b"Let's get started.\r\n".to_vec(), &mut input)
                .is_none()
        );
        assert_eq!(input, b"");

        let output = filter.filter(
            b" Choose the text style that looks best with your terminal\r\n Syntax theme: Monokai Extended\r\n".to_vec(),
            &mut input,
        );

        assert!(output.is_none(), "buffered prompt must remain hidden");
        assert_eq!(input, b"2\r");
    }

    #[test]
    fn non_theme_output_streams_normally() {
        let mut filter = ClaudeThemePromptAutoSelect::default();
        let mut input = Vec::new();
        let output = filter
            .filter(b"normal agent output\r\n".to_vec(), &mut input)
            .expect("normal output is forwarded");

        assert_eq!(output, b"normal agent output\r\n");
        assert!(input.is_empty());
    }
}
