use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event as SseEvent, Sse};
use axum::response::{IntoResponse, Response as AxumResponse};
use futures_util::{StreamExt, stream};
use jeryu_agent_stream::{
    AgentEventBudget, AgentOutputStream, AgentRunStreamKey, AgentTtyEvent, CONTROL_TOPIC, TTY_TOPIC,
};
use jeryu_agentbridge::driver::{
    AgentDriver, AgentEvent, AgentEventSink, AgentRunResult, CommandSpec, DriverError,
};
use jeryu_agentbridge::pty_driver::{AgentControl, PtyAgentDriver};
use jeryu_core::{CreatePullRequestRequest, ForgeError};
use jeryu_readmodel::contracts::WebEvent;
use jeryu_runnerd::{WorkcellLease, WorkcellState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use super::WebState;
use super::surface::serialize_payload;
use super::workcells_support::{
    TypedError, forge_error, manager, normalize_deprecated_host_path, typed_error,
};

const AGENT_RUN_DOCS: &str = "docs/workcell.md#agent-run-control-surface";
const AGENT_RUN_RERUN: &str = "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs";
type AgentRunResponseResult<T> = Result<T, Box<AxumResponse>>;

/// How many raw TTY events one run keeps live for cursor-pull tailing. Past this
/// bound the oldest event is dropped so a long-lived landed session can never
/// grow the in-memory store without limit.
const TTY_RING_CAP: usize = 4096;

/// Live fan-out depth for the per-run raw TTY broadcast that backs the SSE push
/// transport. A subscriber that drains slower than this bound overflows and is
/// handed a `resync` marker so it re-pulls the retained ring rather than stalling.
const TTY_BROADCAST_CAP: usize = 1024;

/// Build a fresh per-run raw TTY broadcast channel and hand back only the sender.
/// Subscribers materialize their own receiver through `subscribe` when a live SSE
/// stream opens, so the channel keeps no idle receiver pinned open between viewers.
fn new_tty_broadcast() -> broadcast::Sender<AgentTtyEvent> {
    broadcast::channel(TTY_BROADCAST_CAP).0
}

/// A bounded, drop-oldest ring of raw TTY events for one agent run.
///
/// New events are appended at the back; once the ring is at capacity the oldest
/// event at the front is evicted. The monotonic per-event `seq` is assigned by
/// the publisher and therefore stays strictly increasing across the whole run
/// even after eviction. `oldest_retained_seq` is the `seq` of the oldest event
/// still held (0 while empty), so a tail reader whose cursor points before it
/// knows part of the byte history rolled off and it must resync.
#[derive(Debug, Clone)]
struct TtyRing {
    events: VecDeque<AgentTtyEvent>,
    cap: usize,
    oldest_retained_seq: u64,
}

impl Default for TtyRing {
    fn default() -> Self {
        Self::new()
    }
}

impl TtyRing {
    fn new() -> Self {
        Self::with_cap(TTY_RING_CAP)
    }

    fn with_cap(cap: usize) -> Self {
        Self {
            events: VecDeque::new(),
            cap: cap.max(1),
            oldest_retained_seq: 0,
        }
    }

    /// Publish one event, evicting the oldest first when the ring is full.
    fn push(&mut self, event: AgentTtyEvent) {
        if self.events.len() >= self.cap {
            self.events.pop_front();
        }
        self.events.push_back(event);
        self.oldest_retained_seq = self.events.front().map_or(0, |event| event.seq);
    }

    fn iter(&self) -> impl Iterator<Item = &AgentTtyEvent> {
        self.events.iter()
    }

    /// Every retained event in publish order (oldest first).
    fn snapshot(&self) -> Vec<AgentTtyEvent> {
        self.events.iter().cloned().collect()
    }

    /// Raw events with `seq > after_seq`, capped at `limit`, oldest first.
    fn tail(&self, after_seq: u64, limit: usize) -> Vec<AgentTtyEvent> {
        self.events
            .iter()
            .filter(|event| event.seq > after_seq)
            .take(limit)
            .cloned()
            .collect()
    }

    /// True when `after_seq` sits before the oldest event still retained, i.e.
    /// the reader fell behind the ring and the events between its cursor and the
    /// oldest retained `seq` have already rolled off. A cursor that lands exactly
    /// on the last evicted `seq` is contiguous with the front and is not lagged.
    fn lagged(&self, after_seq: u64) -> bool {
        after_seq.saturating_add(1) < self.oldest_retained_seq
    }
}

#[derive(Clone, Default)]
pub(crate) struct AgentRunStore {
    inner: Arc<Mutex<AgentRunStoreInner>>,
    next_id: Arc<AtomicU64>,
}

#[derive(Default)]
struct AgentRunStoreInner {
    runs: BTreeMap<String, AgentRunRecord>,
    /// Map from agent run_id to its companion shell run_id.
    shell_companions: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct AgentRunRecord {
    id: String,
    state: AgentRunState,
    io_mode: AgentRunIoMode,
    source: AgentRunSourceSnapshot,
    repo_root: PathBuf,
    program: String,
    args: Vec<String>,
    events: Vec<AgentRunEvent>,
    tty_events: TtyRing,
    /// Live fan-out sender for raw TTY events. `append_event` publishes here right
    /// after the ring push so an open SSE stream sees the same bytes the bounded
    /// ring retains for cursor-pull replay.
    tty_tx: broadcast::Sender<AgentTtyEvent>,
    controls: Vec<AgentRunControlRecord>,
    outcome: Option<AgentRunOutcome>,
    error_code: Option<String>,
    error_message: Option<String>,
    control_tx: Option<Sender<AgentControl>>,
    /// Owning repository `owner/name` for a repo-scoped session run; `None` for
    /// workcell-backed runs. The per-repo agent-runs route filters on this so one
    /// repository's live runs can never leak into another's list.
    repo: Option<String>,
    /// The unique, namespaced session branch the agent works on (never `main`).
    branch: Option<String>,
    /// The latest-`main` oid the session branch was registered at.
    base_oid: Option<String>,
    /// Runner / node identity executing the session.
    runner: Option<String>,
    /// Agent identity that owns the session.
    agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AgentRunState {
    Running,
    Succeeded,
    Failed,
    Exported,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AgentRunIoMode {
    #[default]
    Pty,
    Pipe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentRunStartRequest {
    pub source: AgentRunSource,
    #[serde(default)]
    pub io_mode: AgentRunIoMode,
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub budget: AgentRunBudget,
    #[cfg(test)]
    #[serde(default = "default_true")]
    pub require_cgroup: bool,
}

impl AgentRunStartRequest {
    fn require_cgroup(&self) -> bool {
        #[cfg(test)]
        {
            self.require_cgroup
        }
        #[cfg(not(test))]
        {
            true
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AgentRunSource {
    Repo {
        repo: String,
    },
    LocalPath {
        local_path: PathBuf,
    },
    Scratch {
        name: Option<String>,
    },
    Workcell {
        workcell_id: String,
        runner_epoch: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum AgentRunSourceSnapshot {
    Repo {
        repo: String,
    },
    LocalPath {
        local_path: PathBuf,
    },
    Scratch {
        name: Option<String>,
    },
    Workcell {
        workcell_id: String,
        runner_epoch: u64,
        ci_run_id: Option<String>,
        failed_run_id: Option<String>,
        failed_receipt_id: Option<String>,
        failure_log_digest: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentRunBudget {
    #[serde(default = "default_wall_secs")]
    pub wall_secs: u64,
    #[serde(default = "default_output_bytes")]
    pub output_bytes: usize,
}

impl Default for AgentRunBudget {
    fn default() -> Self {
        Self {
            wall_secs: default_wall_secs(),
            output_bytes: default_output_bytes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AgentControlCommand {
    SendInput { text: String },
    InjectPrompt { text: String },
    Interrupt,
    Terminate,
    ResizePty { cols: u16, rows: u16 },
    RaiseBudget { output_bytes: usize },
}

#[derive(Debug, Clone, Serialize)]
struct AgentRunStartResponse {
    pub agent_run_id: String,
    pub status_url: String,
    pub events_url: String,
    pub control_url: String,
    pub export_pr_url: String,
    pub ws_scope: String,
    pub tty_topic: String,
    pub control_topic: String,
    pub io_mode: AgentRunIoMode,
    pub state: AgentRunState,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AgentRunStatusResponse {
    pub agent_run_id: String,
    pub state: AgentRunState,
    pub io_mode: AgentRunIoMode,
    pub source: AgentRunSourceSnapshot,
    pub repo_root: PathBuf,
    pub program: String,
    pub args: Vec<String>,
    pub events_url: String,
    pub control_url: String,
    pub export_pr_url: String,
    pub ws_scope: String,
    pub tty_topic: String,
    pub control_topic: String,
    pub events: Vec<AgentRunEvent>,
    pub tty_events: Vec<AgentTtyEvent>,
    pub controls: Vec<AgentRunControlRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<AgentRunOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentRunControlResponse {
    pub agent_run_id: String,
    pub accepted: bool,
    pub control_seq: u64,
    pub command: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AgentRunEventsQuery {
    #[serde(default)]
    pub(super) after_seq: Option<u64>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentRunEventsResponse {
    pub agent_run_id: String,
    pub after_seq: u64,
    pub next_after_seq: u64,
    pub limit: usize,
    pub has_more: bool,
    pub events: Vec<AgentRunEvent>,
    pub tty_events: Vec<AgentTtyEvent>,
}

/// Result of `agent_work.tail`: a slice of one run's raw TTY byte stream past a
/// cursor, plus the next cursor and a `lagged` flag a subscriber uses to decide
/// whether it must resync after the drop-oldest ring rolled events off.
#[derive(Debug, Clone, Serialize)]
pub(super) struct AgentRunTailResponse {
    pub agent_run_id: String,
    pub after_seq: u64,
    pub next_after_seq: u64,
    pub oldest_retained_seq: u64,
    pub lagged: bool,
    pub tty_topic: String,
    pub events: Vec<AgentTtyEvent>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentRunExportPrRequest {
    pub owner: String,
    pub repo: String,
    pub author: String,
    #[serde(default)]
    pub branch_suffix: Option<String>,
    #[serde(default)]
    pub target_branch: Option<String>,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentRunExportPrResponse {
    pub agent_run_id: String,
    pub branch: String,
    pub target_branch: String,
    pub pull_request_number: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AgentRunControlRecord {
    pub seq: u64,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AgentRunEvent {
    pub seq: u64,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub timed_out: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub budget_exceeded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AgentRunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub budget_exceeded: bool,
    pub captured_bytes: usize,
    pub enforcement_level: String,
    pub elapsed_ms: u64,
    pub succeeded: bool,
}

struct ResolvedAgentRun {
    source: AgentRunSourceSnapshot,
    repo_root: PathBuf,
    program: PathBuf,
    env: BTreeMap<String, String>,
}

/// Inputs needed to record a freshly-launched repo-scoped session run.
pub(super) struct SessionRecordInit {
    pub run_id: String,
    pub repo: String,
    pub branch: String,
    pub base_oid: String,
    pub runner: String,
    pub agent: String,
    pub program: String,
    pub args: Vec<String>,
    pub workspace: PathBuf,
    /// Live control sender the web terminal steers the PTY agent through. The
    /// matching receiver is handed to the driver thread that supervises the agent.
    pub control_tx: Option<Sender<AgentControl>>,
}

/// The minimal record view a mediated publish needs.
pub(super) struct SessionPublishInfo {
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub base_oid: Option<String>,
    pub state: AgentRunState,
}

/// One row of the per-repo live agent-runs list consumed by the web
/// Active-Agents page (`GET /api/v1/repos/{id}/agent-runs`).
#[derive(Debug, Clone, Serialize)]
pub(super) struct RepoAgentRunRow {
    pub run_id: String,
    pub branch: String,
    pub runner: String,
    pub status: String,
    pub io_mode: AgentRunIoMode,
    pub tty_live: bool,
    pub supported_controls: Vec<String>,
    pub ws_scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Companion shell run id for split terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workcell_id: Option<String>,
}

impl RepoAgentRunRow {
    fn from_record(record: &AgentRunRecord) -> Self {
        let tty_live = record.state == AgentRunState::Running
            && record.io_mode == AgentRunIoMode::Pty
            && record.control_tx.is_some();
        let supported_controls = if record.io_mode == AgentRunIoMode::Pty {
            vec![
                "send_input".to_string(),
                "inject_prompt".to_string(),
                "interrupt".to_string(),
                "terminate".to_string(),
                "resize_pty".to_string(),
                "raise_budget".to_string(),
            ]
        } else {
            Vec::new()
        };
        let workcell_id = match &record.source {
            AgentRunSourceSnapshot::Workcell { workcell_id, .. } => Some(workcell_id.clone()),
            _ => None,
        };
        Self {
            run_id: record.id.clone(),
            branch: record.branch.clone().unwrap_or_default(),
            runner: record.runner.clone().unwrap_or_else(|| "local".to_string()),
            status: agent_run_state_label(record.state).to_string(),
            io_mode: record.io_mode,
            tty_live,
            supported_controls,
            ws_scope: format!("agent_run.{}", record.id),
            agent: record.agent.clone(),
            shell_run_id: None,
            workcell_id,
        }
    }
}

/// Stable lowercase lifecycle label for a run state (matches the serde encoding).
pub(super) fn agent_run_state_label(state: AgentRunState) -> &'static str {
    match state {
        AgentRunState::Running => "running",
        AgentRunState::Succeeded => "succeeded",
        AgentRunState::Failed => "failed",
        AgentRunState::Exported => "exported",
    }
}

// Keep route orchestration, state mutation, and PR export independently
// reviewable while preserving this module as the single public surface.
mod export;
mod handlers;
mod store;
#[cfg(test)]
mod tail_tests;

#[cfg(test)]
pub(super) use handlers::AgentTtyStreamQuery;
pub(super) use handlers::{
    PtyBackend, SessionAgentSpawn, control, events, export_pr, list, mcp_control, mcp_events,
    mcp_export_pr, mcp_start, mcp_status, mcp_tail, shell, spawn_session_agent, start, status,
    tty_stream,
};
#[cfg(test)]
pub(super) use store::{test_raw_tty_event, tty_broadcast_capacity};

pub(super) fn origin_base_url(headers: &HeaderMap) -> String {
    match headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.trim().is_empty())
    {
        Some(host) => format!("http://{host}"),
        None => String::new(),
    }
}

fn derive_allowed_prefixes(allowed_paths: &[PathBuf], workspace_root: &Path) -> Vec<String> {
    let prefixes: Vec<String> = allowed_paths
        .iter()
        .filter_map(|path| path.strip_prefix(workspace_root).ok())
        .map(|relative| relative.to_string_lossy().to_string())
        .collect();
    let has_specific = prefixes.iter().any(|prefix| !prefix.is_empty());
    if has_specific {
        prefixes
            .into_iter()
            .filter(|prefix| !prefix.is_empty())
            .collect()
    } else {
        prefixes
    }
}

fn normalize_pr_base(ref_name: String) -> String {
    let without_heads = ref_name
        .strip_prefix("refs/heads/")
        .unwrap_or(ref_name.as_str());
    without_heads
        .strip_prefix("origin/")
        .unwrap_or(without_heads)
        .to_string()
}

pub(super) fn snapshot_event(state: &WebState, agent_run_id: &str) -> Option<WebEvent> {
    let status = state.agent_runs.status(agent_run_id)?;
    let payload = serialize_payload(&status).ok()?;
    Some(WebEvent {
        seq: state.ws.next_seq(),
        timestamp: super::server_time(),
        scope: format!("agent_run.{agent_run_id}"),
        kind: "agent_run.snapshot".to_string(),
        entity: agent_run_id.to_string(),
        summary: format!("agent run '{}' is {:?}", agent_run_id, status.state),
        payload,
    })
}

struct RecordingSink {
    store: AgentRunStore,
    run_id: String,
}

impl AgentEventSink for RecordingSink {
    fn emit(&self, ev: AgentEvent) {
        self.store.append_event(&self.run_id, ev.into());
    }
}

struct AgentRunEventInput {
    kind: &'static str,
    stream: Option<&'static str>,
    text: Option<String>,
    pid: Option<u32>,
    used: Option<usize>,
    limit: Option<usize>,
    exit_code: Option<i32>,
    timed_out: bool,
    budget_exceeded: bool,
}

impl AgentRunEventInput {
    fn into_event(self, seq: u64) -> AgentRunEvent {
        AgentRunEvent {
            seq,
            kind: self.kind.to_string(),
            stream: self.stream.map(ToString::to_string),
            text: self.text,
            pid: self.pid,
            used: self.used,
            limit: self.limit,
            exit_code: self.exit_code,
            timed_out: self.timed_out,
            budget_exceeded: self.budget_exceeded,
        }
    }
}

impl From<AgentEvent> for AgentRunEventInput {
    fn from(value: AgentEvent) -> Self {
        match value {
            AgentEvent::Started { pid } => Self {
                kind: "started",
                stream: None,
                text: None,
                pid: Some(pid),
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Stdout(text) => Self {
                kind: "tty",
                stream: Some("stdout"),
                text: Some(text),
                pid: None,
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Stderr(text) => Self {
                kind: "tty",
                stream: Some("stderr"),
                text: Some(text),
                pid: None,
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Budget { used, limit } => Self {
                kind: "budget",
                stream: None,
                text: None,
                pid: None,
                used: Some(used),
                limit: Some(limit),
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Finished {
                exit_code,
                timed_out,
                budget_exceeded,
            } => Self {
                kind: "finished",
                stream: None,
                text: None,
                pid: None,
                used: None,
                limit: None,
                exit_code,
                timed_out,
                budget_exceeded,
            },
        }
    }
}

impl AgentRunOutcome {
    fn from_result(value: AgentRunResult) -> Self {
        let succeeded = value.succeeded();
        Self {
            exit_code: value.exit_code,
            timed_out: value.timed_out,
            budget_exceeded: value.budget_exceeded,
            captured_bytes: value.captured_bytes,
            enforcement_level: value.enforcement_level,
            elapsed_ms: u64::try_from(value.elapsed.as_millis()).unwrap_or(u64::MAX),
            succeeded,
        }
    }
}

fn parse_agent_body<T: for<'de> Deserialize<'de>>(
    body: &Bytes,
    purpose: &'static str,
) -> AgentRunResponseResult<T> {
    serde_json::from_slice(body).map_err(|err| {
        let message = err.to_string();
        boxed_agent_run_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_run_invalid_request",
            purpose,
            &message,
            &[
                "send a JSON body that matches the agent-run route schema",
                "use the typed MCP/API surface to build the request",
            ],
            "fix the request body, then rerun the agent_runs proof lane",
        )
    })
}

fn parse_control_body(body: &Bytes) -> AgentRunResponseResult<AgentControlCommand> {
    let value: Value = parse_agent_body(body, "send control to an agent run")?;
    let command_value = value.get("command").unwrap_or(&value).clone();
    serde_json::from_value(command_value).map_err(|err| {
        let message = err.to_string();
        boxed_agent_run_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_run_invalid_control",
            "send control to an agent run",
            &message,
            &[
                "send one of send_input, inject_prompt, interrupt, terminate, resize_pty, or raise_budget",
                "use io_mode pty for live controls",
            ],
            "fix the control body, then rerun the agent_runs proof lane",
        )
    })
}

fn map_control(command: &AgentControlCommand) -> AgentControl {
    match command {
        AgentControlCommand::SendInput { text } => {
            AgentControl::SendInput(text.clone().into_bytes())
        }
        AgentControlCommand::InjectPrompt { text } => AgentControl::InjectPrompt(text.clone()),
        AgentControlCommand::Interrupt => AgentControl::Interrupt,
        AgentControlCommand::Terminate => AgentControl::Terminate,
        AgentControlCommand::ResizePty { cols, rows } => AgentControl::ResizePty {
            rows: *rows,
            cols: *cols,
        },
        AgentControlCommand::RaiseBudget { output_bytes } => {
            AgentControl::RaiseBudget(*output_bytes)
        }
    }
}

fn command_name(command: &AgentControlCommand) -> &'static str {
    match command {
        AgentControlCommand::SendInput { .. } => "send_input",
        AgentControlCommand::InjectPrompt { .. } => "inject_prompt",
        AgentControlCommand::Interrupt => "interrupt",
        AgentControlCommand::Terminate => "terminate",
        AgentControlCommand::ResizePty { .. } => "resize_pty",
        AgentControlCommand::RaiseBudget { .. } => "raise_budget",
    }
}

fn driver_error_parts(err: DriverError) -> (&'static str, String) {
    match err {
        DriverError::Workspace(reason) => ("agent_run_workspace_denied", reason),
        DriverError::Policy(reason) => ("agent_run_policy_denied", reason),
        DriverError::SandboxUnavailable(reason) => ("agent_run_sandbox_unavailable", reason),
        DriverError::Supervision(reason) => ("agent_run_supervision_failed", reason),
    }
}

fn agent_run_not_found(agent_run_id: &str) -> AxumResponse {
    let message = format!("agent run {agent_run_id} was not found");
    agent_run_typed_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "inspect an agent run",
        &message,
        &[
            "start an agent run before asking for its status",
            "reload the agent-runs list and retry with a live id",
        ],
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    )
}

fn agent_run_workcell_not_found(workcell_id: &str) -> Box<AxumResponse> {
    let message = format!("workcell {workcell_id} was not found");
    boxed_agent_run_typed_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "start an agent run from a failed-CI workcell",
        &message,
        &[
            "hold a failed workcell before starting the repair agent",
            "reload the workcells list and retry with a live id",
        ],
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    )
}

fn agent_run_unavailable(
    code: &'static str,
    purpose: &'static str,
    reason: &str,
) -> Box<AxumResponse> {
    boxed_agent_run_typed_error(
        StatusCode::FAILED_DEPENDENCY,
        code,
        purpose,
        reason,
        &[
            "start from a held failed-CI workcell",
            "wire the missing workspace allocator before enabling this source",
        ],
        AGENT_RUN_RERUN,
    )
}

fn agent_run_path_denied(reason: &'static str) -> Box<AxumResponse> {
    boxed_agent_run_typed_error(
        StatusCode::FORBIDDEN,
        "agent_run_path_denied",
        "start an agent run inside a workcell repo slice",
        reason,
        &[
            "stage the agent command under the selected repo root",
            "reclaim the workcell with a lease that covers the requested path",
        ],
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    )
}

fn boxed_agent_run_typed_error(
    status: StatusCode,
    code: &'static str,
    purpose: &'static str,
    reason: &str,
    common_fixes: &'static [&'static str],
    repair_hint: &'static str,
) -> Box<AxumResponse> {
    Box::new(agent_run_typed_error(
        status,
        code,
        purpose,
        reason,
        common_fixes,
        repair_hint,
    ))
}

fn agent_run_typed_error(
    status: StatusCode,
    code: &'static str,
    purpose: &'static str,
    reason: &str,
    common_fixes: &'static [&'static str],
    repair_hint: &'static str,
) -> AxumResponse {
    typed_error(TypedError {
        status,
        code,
        purpose,
        reason,
        common_fixes,
        docs_url: AGENT_RUN_DOCS,
        repair_hint,
        message: reason,
    })
}

fn default_wall_secs() -> u64 {
    7_200
}

fn default_output_bytes() -> usize {
    20_971_520
}

#[cfg(test)]
fn default_true() -> bool {
    true
}

fn is_false(value: &bool) -> bool {
    !*value
}
