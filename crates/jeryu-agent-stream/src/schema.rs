use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Output event topic.
pub const TTY_TOPIC: &str = "jeryu.agent.tty.v1";
/// Control command topic.
pub const CONTROL_TOPIC: &str = "jeryu.agent.control.v1";
/// Current event schema version.
pub const SCHEMA_VERSION: &str = "jeryu.agent.stream.v1";

/// Stream direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStreamDirection {
    /// Tool output emitted from the running process.
    Output,
    /// Operator/user control input sent to the process.
    Control,
}

/// Logical output stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutputStream {
    /// PTY stream.
    Pty,
    /// Standard output when available.
    Stdout,
    /// Standard error when available.
    Stderr,
    /// Budget or lifecycle metadata.
    Event,
}

/// Budget fields carried on output and exit events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEventBudget {
    /// Wall-clock limit in seconds.
    pub wall_secs: u64,
    /// Output limit in bytes.
    pub output_bytes: u64,
    /// Bytes emitted so far.
    pub used_output_bytes: u64,
}

/// Broker-compatible TTY/output event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTtyEvent {
    /// Schema version.
    pub schema_version: String,
    /// Event id.
    pub event_id: String,
    /// Monotonic per-run sequence.
    pub seq: u64,
    /// Event timestamp in epoch milliseconds.
    pub occurred_at_ms: u64,
    /// Optional repo slug.
    pub repo: Option<String>,
    /// Workcell id.
    pub workcell_id: String,
    /// Agent run id.
    pub agent_run_id: String,
    /// Agent tool id.
    pub agent: String,
    /// Model name passed through by caller/config.
    pub model: String,
    /// Direction.
    pub direction: AgentStreamDirection,
    /// Stream name.
    pub stream: AgentOutputStream,
    /// UTF-8 text chunk where available.
    pub text: Option<String>,
    /// Raw bytes encoded by the adapter, if text is unavailable.
    pub bytes_b64: Option<String>,
    /// True when the payload was truncated.
    pub truncated: bool,
    /// Budget state.
    pub budget: Option<AgentEventBudget>,
    /// Exit code on final events.
    pub exit_code: Option<i32>,
    /// Sandbox enforcement level.
    pub enforcement_level: Option<String>,
}

impl AgentTtyEvent {
    /// Create a text output event.
    #[must_use]
    pub fn text(
        seq: u64,
        occurred_at_ms: u64,
        run: &AgentRunStreamKey,
        stream: AgentOutputStream,
        text: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            event_id: Uuid::new_v4().to_string(),
            seq,
            occurred_at_ms,
            repo: run.repo.clone(),
            workcell_id: run.workcell_id.clone(),
            agent_run_id: run.agent_run_id.clone(),
            agent: run.agent.clone(),
            model: run.model.clone(),
            direction: AgentStreamDirection::Output,
            stream,
            text: Some(text.into()),
            bytes_b64: None,
            truncated: false,
            budget: None,
            exit_code: None,
            enforcement_level: None,
        }
    }

    /// Create a final exit event.
    #[must_use]
    pub fn finished(
        seq: u64,
        occurred_at_ms: u64,
        run: &AgentRunStreamKey,
        exit_code: Option<i32>,
        enforcement_level: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            event_id: Uuid::new_v4().to_string(),
            seq,
            occurred_at_ms,
            repo: run.repo.clone(),
            workcell_id: run.workcell_id.clone(),
            agent_run_id: run.agent_run_id.clone(),
            agent: run.agent.clone(),
            model: run.model.clone(),
            direction: AgentStreamDirection::Output,
            stream: AgentOutputStream::Event,
            text: None,
            bytes_b64: None,
            truncated: false,
            budget: None,
            exit_code,
            enforcement_level: Some(enforcement_level.into()),
        }
    }
}

/// Stable key fields repeated on stream events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunStreamKey {
    /// Optional repo slug.
    pub repo: Option<String>,
    /// Workcell id.
    pub workcell_id: String,
    /// Agent run id.
    pub agent_run_id: String,
    /// Agent tool id.
    pub agent: String,
    /// Model name.
    pub model: String,
}

/// Control command sent to a running or resumable agent run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentControlCommand {
    /// Send raw text to live stdin.
    StdinText { text: String },
    /// Start a continuation round after exit/block.
    ContinuePrompt { prompt: String },
    /// Send an interrupt signal.
    Interrupt,
    /// Terminate the run.
    Terminate,
    /// Resize PTY.
    ResizePty { cols: u16, rows: u16 },
}

impl AgentControlCommand {
    /// Stable control kind label.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::StdinText { .. } => "stdin_text",
            Self::ContinuePrompt { .. } => "continue_prompt",
            Self::Interrupt => "interrupt",
            Self::Terminate => "terminate",
            Self::ResizePty { .. } => "resize_pty",
        }
    }
}

/// Control envelope keyed by `agent_run_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentControlEnvelope {
    /// Schema version.
    pub schema_version: String,
    /// Control id.
    pub event_id: String,
    /// Run id.
    pub agent_run_id: String,
    /// Command.
    pub command: AgentControlCommand,
}

impl AgentControlEnvelope {
    /// Create a control envelope.
    #[must_use]
    pub fn new(agent_run_id: impl Into<String>, command: AgentControlCommand) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            event_id: Uuid::new_v4().to_string(),
            agent_run_id: agent_run_id.into(),
            command,
        }
    }
}
