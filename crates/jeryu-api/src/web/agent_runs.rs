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
use super::workcells_support::{TypedError, forge_error, manager, typed_error};

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

pub(super) async fn start(State(state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    let request: AgentRunStartRequest = match parse_agent_body(&body, "start an agent run") {
        Ok(request) => request,
        Err(response) => return *response,
    };
    match start_request(state, request) {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(response) => *response,
    }
}

fn start_request(
    state: Arc<WebState>,
    request: AgentRunStartRequest,
) -> AgentRunResponseResult<AgentRunStartResponse> {
    let resolved = resolve_agent_run_source(&state, &request)?;

    let agent_run_id = state.agent_runs.allocate_id();
    let (control_tx, control_rx) = mpsc::channel::<AgentControl>();
    let control = if request.io_mode == AgentRunIoMode::Pty {
        Some(control_tx)
    } else {
        None
    };
    let spec = CommandSpec {
        program: resolved.program.to_string_lossy().to_string(),
        args: request.args.clone(),
        env: resolved.env,
    };
    let timeout = Duration::from_secs(request.budget.wall_secs.clamp(1, 86_400));
    let output_budget = request.budget.output_bytes.clamp(1, 128 * 1024 * 1024);
    state.agent_runs.insert(AgentRunRecord {
        id: agent_run_id.clone(),
        state: AgentRunState::Running,
        io_mode: request.io_mode,
        source: resolved.source,
        repo_root: resolved.repo_root.clone(),
        program: spec.program.clone(),
        args: spec.args.clone(),
        events: Vec::new(),
        tty_events: TtyRing::new(),
        tty_tx: new_tty_broadcast(),
        controls: Vec::new(),
        outcome: None,
        error_code: None,
        error_message: None,
        control_tx: control,
        repo: None,
        branch: None,
        base_oid: None,
        runner: None,
        agent: None,
    });

    if let Some(prompt) = request.prompt.clone()
        && request.io_mode == AgentRunIoMode::Pty
    {
        let _ = state
            .agent_runs
            .control_sender(&agent_run_id)
            .and_then(|tx| tx.send(AgentControl::InjectPrompt(prompt)).ok());
    }

    spawn_driver_thread(DriverThreadInit {
        store: state.agent_runs.clone(),
        run_id: agent_run_id.clone(),
        repo_root: resolved.repo_root,
        spec,
        io_mode: request.io_mode,
        backend: PtyBackend::Native,
        docker_fallback: None,
        timeout,
        output_budget,
        require_cgroup: request.require_cgroup(),
        control_rx,
    });

    Ok(AgentRunStartResponse {
        agent_run_id: agent_run_id.clone(),
        status_url: format!("/api/v1/agent-runs/{agent_run_id}"),
        events_url: format!("/api/v1/agent-runs/{agent_run_id}/events"),
        control_url: format!("/api/v1/agent-runs/{agent_run_id}/control"),
        export_pr_url: format!("/api/v1/agent-runs/{agent_run_id}/export_pr"),
        ws_scope: format!("agent_run.{agent_run_id}"),
        tty_topic: TTY_TOPIC.to_string(),
        control_topic: CONTROL_TOPIC.to_string(),
        io_mode: request.io_mode,
        state: AgentRunState::Running,
    })
}

/// Everything one driver thread needs to supervise a real child against a run.
/// Both the public agent-run route and the repo-scoped session launch hand this
/// to [`spawn_driver_thread`], so the two share the exact same supervision path:
/// a [`RecordingSink`] feeds `append_event`/`publish_tty`, and `complete` records
/// the terminal outcome.
struct DriverThreadInit {
    store: AgentRunStore,
    run_id: String,
    repo_root: PathBuf,
    spec: CommandSpec,
    io_mode: AgentRunIoMode,
    /// Which PTY execution backend supervises the child. `Native` runs the program
    /// inside the in-process kernel sandbox; `DockerHost` runs an unsandboxed host
    /// `docker run ...` (the container is the jail). Only meaningful for the Pty
    /// io_mode; the Pipe path always uses the native sandbox driver.
    backend: PtyBackend,
    /// Auto-mode docker fallback. When the `Native` backend returns
    /// `sandbox_unavailable` (the host blocks the unprivileged-userns sandbox) and
    /// this carries a docker command, the same run retries on the docker host-PTY
    /// backend instead of failing — the `auto` runtime selector's whole point.
    docker_fallback: Option<CommandSpec>,
    timeout: Duration,
    output_budget: usize,
    require_cgroup: bool,
    control_rx: mpsc::Receiver<AgentControl>,
}

/// Which PTY backend a launched agent runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PtyBackend {
    /// In-process kernel-sandbox PTY (the existing [`PtyAgentDriver::run`] path).
    Native,
    /// Host `docker run ...` on a PTY; the container engine confines the agent and
    /// the host process is unsandboxed by design (see [`PtyAgentDriver::run_host_pty`]).
    DockerHost,
}

/// Drive one agent child to completion on a background thread, streaming its TTY
/// output into the run's tty ring + broadcast through a [`RecordingSink`]. The
/// Pty path gives the child a controlling terminal and applies live control
/// commands; the Pipe path supervises over pipes. Either way the terminal
/// outcome lands through `store.complete`.
fn spawn_driver_thread(init: DriverThreadInit) {
    let DriverThreadInit {
        store,
        run_id,
        repo_root,
        spec,
        io_mode,
        backend,
        docker_fallback,
        timeout,
        output_budget,
        require_cgroup,
        control_rx,
    } = init;
    std::thread::spawn(move || {
        let sink = RecordingSink {
            store: store.clone(),
            run_id: run_id.clone(),
        };
        let driver = PtyAgentDriver::new(timeout, output_budget);
        let result = match (io_mode, backend) {
            // Docker-backed live PTY: the container is the jail, so the host
            // `docker run ...` runs unsandboxed on a controlling terminal.
            (AgentRunIoMode::Pty, PtyBackend::DockerHost) => {
                driver.run_host_pty(&repo_root, &spec, &sink, &control_rx)
            }
            (AgentRunIoMode::Pty, PtyBackend::Native) => {
                let native = driver.clone().with_require_cgroup(require_cgroup).run(
                    &repo_root,
                    &spec,
                    &sink,
                    &control_rx,
                );
                match (native, docker_fallback) {
                    // Auto fallback: the host blocked the kernel sandbox, so retry
                    // the same run on the docker host-PTY backend.
                    (Err(DriverError::SandboxUnavailable(_)), Some(docker_spec)) => {
                        driver.run_host_pty(&repo_root, &docker_spec, &sink, &control_rx)
                    }
                    (other, _) => other,
                }
            }
            (AgentRunIoMode::Pipe, _) => AgentDriver::new(timeout, output_budget)
                .with_require_cgroup(require_cgroup)
                .run(&repo_root, &spec, &sink),
        };
        store.complete(&run_id, result);
    });
}

/// Inputs the repo-scoped session launch hands to [`spawn_session_agent`] to put
/// the selected agent on a controlling PTY against the session workspace.
pub(super) struct SessionAgentSpawn {
    /// The recorded session run the TTY stream is keyed to.
    pub run_id: String,
    /// The materialized session checkout the agent runs inside (its cwd). For the
    /// native backend this is also the agent's cwd; for the docker backend it is
    /// where the host `docker run` process runs (and the workspace it bind-mounts).
    pub workspace: PathBuf,
    /// The launch command the driver runs. `None` means the agent could not be
    /// resolved (a missing host binary, or `docker` absent from PATH), in which
    /// case the run records one graceful "not available" line instead of starting.
    pub spec: Option<CommandSpec>,
    /// Which PTY backend supervises the child (native sandbox vs. host docker).
    pub backend: PtyBackend,
    /// Auto-mode docker fallback command: when `backend` is `Native` and native
    /// returns `sandbox_unavailable`, the run retries on the docker host-PTY
    /// backend with this command instead of failing.
    pub docker_fallback: Option<CommandSpec>,
    /// Live control sender already recorded against the run, moved into the driver.
    pub control_rx: mpsc::Receiver<AgentControl>,
    /// Wall-clock budget for the session agent.
    pub timeout: Duration,
    /// Captured-output byte budget for the session agent.
    pub output_budget: usize,
    /// Whether enforced cgroup-v2 limits are required (false only under test, and
    /// only consulted by the native backend; the docker backend ignores it).
    pub require_cgroup: bool,
}

/// Launch the selected agent for a repo-scoped session on a controlling PTY,
/// wiring its raw terminal output into the recorded run exactly like the public
/// agent-run route. The native backend runs the agent inside the in-process kernel
/// sandbox; the docker backend runs `docker run ...` on the host PTY and the
/// container is the jail. Either way the agent works against the session checkout
/// (its cwd for native, its `/workspace` bind mount for docker) on its own branch.
///
/// When the agent could not be resolved (`spec` is `None`) — a missing host binary
/// or `docker` absent from PATH — rather than fail the whole New Session request,
/// record one clear TTY line so the web terminal shows why the agent never started,
/// and mark the run finished.
pub(super) fn spawn_session_agent(store: &AgentRunStore, spawn: SessionAgentSpawn) {
    let SessionAgentSpawn {
        run_id,
        workspace,
        spec,
        backend,
        docker_fallback,
        control_rx,
        timeout,
        output_budget,
        require_cgroup,
    } = spawn;

    let Some(spec) = spec else {
        store.note_agent_unavailable(&run_id);
        return;
    };
    spawn_driver_thread(DriverThreadInit {
        store: store.clone(),
        run_id,
        repo_root: workspace,
        spec,
        io_mode: AgentRunIoMode::Pty,
        backend,
        docker_fallback,
        timeout,
        output_budget,
        require_cgroup,
        control_rx,
    });
}

pub(super) async fn list(State(state): State<Arc<WebState>>) -> Json<Vec<AgentRunStatusResponse>> {
    Json(state.agent_runs.list())
}

pub(super) async fn status(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
) -> AxumResponse {
    match state.agent_runs.status(&agent_run_id) {
        Some(response) => Json(response).into_response(),
        None => agent_run_not_found(&agent_run_id),
    }
}

pub(super) async fn control(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let command = match parse_control_body(&body) {
        Ok(command) => command,
        Err(response) => return *response,
    };
    match state.agent_runs.send_control(&agent_run_id, command) {
        Ok(response) => Json(response).into_response(),
        Err(response) => *response,
    }
}

pub(super) async fn events(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    Query(query): Query<AgentRunEventsQuery>,
) -> AxumResponse {
    match state.agent_runs.events(&agent_run_id, query) {
        Some(response) => Json(response).into_response(),
        None => agent_run_not_found(&agent_run_id),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct AgentTtyStreamQuery {
    #[serde(default)]
    pub(super) after_seq: Option<u64>,
}

/// Server-Sent-Events push transport for one run's raw TTY byte stream.
///
/// `GET /api/v1/agent-runs/{id}/tty/stream?after_seq=N` is the jpmc-subscribable
/// live transport: an outside service opens it once and is pushed raw bytes as they
/// reach the single `append_event` publish point, with no cursor-polling of
/// `agent_work.tail`. The handler first replays the retained ring slice past
/// `after_seq` (so a reconnect catches up byte-for-byte), then hands over the live
/// broadcast. Each `data:` frame carries the same JSON shape a tail event does
/// (`seq` + `stream` + `bytes_b64`). It mirrors the WS scope-membership rule: an
/// unknown or non-member run yields the same typed not-found the WS snapshot path
/// ignores. When the live broadcast overflows for a slow subscriber, one
/// `event: resync` frame carrying `oldest_retained_seq` is pushed so the client
/// re-pulls the ring through `agent_work.tail` instead of stalling or erroring.
pub(super) async fn tty_stream(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    Query(query): Query<AgentTtyStreamQuery>,
) -> AxumResponse {
    let after_seq = query.after_seq.unwrap_or(0);
    let Some((receiver, replay)) = state.agent_runs.tty_stream_start(&agent_run_id, after_seq)
    else {
        return agent_run_not_found(&agent_run_id);
    };

    // Replay prelude: a resync marker when the cursor already fell behind the ring,
    // then every retained event past the cursor, oldest first.
    let mut prelude: Vec<Result<SseEvent, Infallible>> = Vec::new();
    if replay.lagged {
        prelude.push(Ok(tty_resync_frame(replay.oldest_retained_seq)));
    }
    for event in &replay.events {
        prelude.push(Ok(tty_data_frame(event)));
    }

    // Live tail: events past the replay cursor, with broadcast overflow turned into
    // a single resync marker so a lagged subscriber is told to re-pull, never stalled.
    let store = state.agent_runs.clone();
    let live = stream::unfold(
        (receiver, replay.next_after_seq, store, agent_run_id),
        |(mut receiver, cursor, store, run_id)| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if event.seq <= cursor {
                            continue;
                        }
                        let next_cursor = event.seq;
                        let frame = Ok(tty_data_frame(&event));
                        return Some((frame, (receiver, next_cursor, store, run_id)));
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let oldest = store.oldest_retained_tty_seq(&run_id);
                        let frame = Ok(tty_resync_frame(oldest));
                        return Some((frame, (receiver, cursor, store, run_id)));
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );

    Sse::new(stream::iter(prelude).chain(live)).into_response()
}

/// Encode one raw TTY event as an SSE `data:` frame. The payload is the event's
/// own JSON, so a subscriber reads the same `seq` + `stream` + `bytes_b64` shape a
/// cursor-pull tail returns and raw non-UTF8 bytes ride through base64 intact.
fn tty_data_frame(event: &AgentTtyEvent) -> SseEvent {
    let payload = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
    SseEvent::default().data(payload)
}

/// Build the `resync` marker frame a lagged subscriber receives, carrying the
/// oldest `seq` the ring still retains as the floor to re-pull from.
fn tty_resync_frame(oldest_retained_seq: u64) -> SseEvent {
    SseEvent::default()
        .event("resync")
        .data(json!({ "oldest_retained_seq": oldest_retained_seq }).to_string())
}

pub(super) async fn export_pr(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> AxumResponse {
    let request: AgentRunExportPrRequest =
        match parse_agent_body(&body, "export an agent run into a pull request") {
            Ok(request) => request,
            Err(response) => return *response,
        };
    match export_workcell_agent_run(&state, &agent_run_id, request, &origin_base_url(&headers)) {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(response) => *response,
    }
}

/// `POST /api/v1/agent-runs/{id}/shell` — spawn a companion shell in the same
/// workspace as the given run. Returns the companion run's id and URLs so the
/// frontend can mount a second terminal pane for free-form operator interaction.
#[derive(Debug, Serialize)]
struct CompanionShellResponse {
    shell_run_id: String,
    status_url: String,
    tty_stream_url: String,
    control_url: String,
}

pub(super) async fn shell(
    State(state): State<Arc<WebState>>,
    AxumPath(parent_run_id): AxumPath<String>,
) -> AxumResponse {
    // Look up the parent run's workspace.
    let workspace = {
        let inner = state.agent_runs.inner.lock().expect("runs mutex");
        inner.runs.get(&parent_run_id).map(|r| r.repo_root.clone())
    };
    let Some(workspace) = workspace else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("agent run {parent_run_id} not found") }
            })),
        ).into_response();
    };

    // Allocate a new run for the companion shell.
    let shell_id = state.agent_runs.allocate_id();
    let (control_tx, control_rx) = std::sync::mpsc::channel();

    let repo_name = {
        let inner = state.agent_runs.inner.lock().expect("runs mutex");
        inner
            .runs
            .get(&parent_run_id)
            .and_then(|r| r.repo.clone())
            .unwrap_or_default()
    };

    state.agent_runs.insert_session(SessionRecordInit {
        run_id: shell_id.clone(),
        repo: repo_name,
        branch: String::new(),
        base_oid: String::new(),
        runner: "local".to_string(),
        agent: "shell".to_string(),
        program: "/bin/bash".to_string(),
        args: vec!["--login".to_string()],
        workspace: workspace.clone(),
        control_tx: Some(control_tx),
    });

    let spec = CommandSpec {
        program: "/bin/bash".to_string(),
        args: vec!["--login".to_string()],
        env: Default::default(),
    };

    spawn_session_agent(
        &state.agent_runs,
        SessionAgentSpawn {
            run_id: shell_id.clone(),
            workspace,
            spec: Some(spec),
            backend: PtyBackend::Native,
            docker_fallback: None,
            control_rx,
            timeout: std::time::Duration::from_secs(7200),
            output_budget: 20_971_520,
            require_cgroup: false,
        },
    );

    (
        StatusCode::CREATED,
        Json(CompanionShellResponse {
            status_url: format!("/api/v1/agent-runs/{shell_id}"),
            tty_stream_url: format!("/api/v1/agent-runs/{shell_id}/tty/stream"),
            control_url: format!("/api/v1/agent-runs/{shell_id}/control"),
            shell_run_id: shell_id,
        }),
    )
        .into_response()
}

pub(super) fn mcp_start(state: Arc<WebState>, args: Value) -> Result<Value, String> {
    let request: AgentRunStartRequest =
        serde_json::from_value(args).map_err(|err| err.to_string())?;
    let response = start_request(state, request).map_err(|_| {
        "agent_work.start failed; use the REST route for typed repair details".to_string()
    })?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(super) fn mcp_status(state: &Arc<WebState>, args: &Value) -> Result<Value, String> {
    let run_id = required_run_id(args)?;
    let response = state
        .agent_runs
        .status(&run_id)
        .ok_or_else(|| format!("agent run {run_id} was not found"))?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(super) fn mcp_control(state: &Arc<WebState>, args: Value) -> Result<Value, String> {
    let run_id = required_run_id(&args)?;
    let command_value = args
        .get("command")
        .cloned()
        .ok_or_else(|| "agent_work.control requires command".to_string())?;
    let command: AgentControlCommand =
        serde_json::from_value(command_value).map_err(|err| err.to_string())?;
    let response = state
        .agent_runs
        .send_control(&run_id, command)
        .map_err(|_| "agent_work.control failed; use REST for typed repair details".to_string())?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(super) fn mcp_events(state: &Arc<WebState>, args: &Value) -> Result<Value, String> {
    let run_id = required_run_id(args)?;
    let query = AgentRunEventsQuery {
        after_seq: args.get("after_seq").and_then(Value::as_u64),
        limit: args
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok()),
    };
    let response = state
        .agent_runs
        .events(&run_id, query)
        .ok_or_else(|| format!("agent run {run_id} was not found"))?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(super) fn mcp_tail(state: &Arc<WebState>, args: &Value) -> Result<Value, String> {
    let run_id = required_run_id(args)?;
    let after_seq = args.get("after_seq").and_then(Value::as_u64).unwrap_or(0);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    let response = state
        .agent_runs
        .tail_tty(&run_id, after_seq, limit)
        .ok_or_else(|| format!("agent run {run_id} was not found"))?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

pub(super) fn mcp_export_pr(state: &Arc<WebState>, args: Value) -> Result<Value, String> {
    let run_id = required_run_id(&args)?;
    let request: AgentRunExportPrRequest =
        serde_json::from_value(args).map_err(|err| err.to_string())?;
    let response = export_workcell_agent_run(
        state,
        &run_id,
        request,
        &crate::ci_bridge::default_origin_base_url(),
    )
    .map_err(|_| "agent_work.export_pr failed; use REST for typed repair details".to_string())?;
    serde_json::to_value(response).map_err(|err| err.to_string())
}

fn required_run_id(args: &Value) -> Result<String, String> {
    args.get("agent_run_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| "agent_run_id is required".to_string())
}

fn resolve_agent_run_source(
    state: &Arc<WebState>,
    request: &AgentRunStartRequest,
) -> AgentRunResponseResult<ResolvedAgentRun> {
    match &request.source {
        AgentRunSource::Workcell {
            workcell_id,
            runner_epoch,
        } => resolve_workcell_source(state, request, workcell_id, *runner_epoch),
        AgentRunSource::Repo { repo } => {
            let reason = format!("repo source {repo} needs a checkout allocator before launch");
            Err(agent_run_unavailable(
                "agent_run_repo_source_unavailable",
                "start an agent run from a repository",
                &reason,
            ))
        }
        AgentRunSource::LocalPath { local_path } => {
            let reason = format!(
                "local_path source {} is not enabled for the public agent-run route",
                local_path.display()
            );
            Err(agent_run_unavailable(
                "agent_run_local_path_unavailable",
                "start an agent run from a local path",
                &reason,
            ))
        }
        AgentRunSource::Scratch { name } => {
            let reason = format!(
                "scratch source {} needs a workspace allocator before launch",
                name.as_deref().unwrap_or("unnamed")
            );
            Err(agent_run_unavailable(
                "agent_run_scratch_unavailable",
                "start an agent run from a scratch workspace",
                &reason,
            ))
        }
    }
}

fn resolve_workcell_source(
    state: &Arc<WebState>,
    request: &AgentRunStartRequest,
    workcell_id: &str,
    runner_epoch: u64,
) -> AgentRunResponseResult<ResolvedAgentRun> {
    let lease = match manager(state).workcell(workcell_id).cloned() {
        Some(lease) => lease,
        None => return Err(agent_run_workcell_not_found(workcell_id)),
    };
    if lease.runner_epoch != runner_epoch {
        return Err(boxed_agent_run_typed_error(
            StatusCode::CONFLICT,
            "workcell_epoch_fenced",
            "start an agent run from a failed-CI workcell",
            "request runner_epoch did not match the active workcell epoch",
            &[
                "reload workcell status and retry with the active runner_epoch",
                "discard stale failed-CI repair requests",
            ],
            "the agent run request used a stale workcell epoch",
        ));
    }
    if !matches!(lease.state, WorkcellState::Held | WorkcellState::Repairing) {
        return Err(boxed_agent_run_typed_error(
            StatusCode::CONFLICT,
            "agent_run_workcell_state_denied",
            "start an agent run from a failed-CI workcell",
            "the workcell is not held or repairing",
            &[
                "freeze the failed CI tree before launching the repair agent",
                "use /api/v1/workcells/{id}/run_agent for deterministic claimed-cell commands",
            ],
            "start from a held or repairing workcell, then rerun the agent_runs proof lane",
        ));
    }
    let repo_root = select_repo_root(&lease, request.repo_root.as_deref())?;
    let program = resolve_program(&repo_root, &request.program)?;
    let mut env = request.env.clone();
    inject_workcell_env(&mut env, &lease);
    if request.io_mode == AgentRunIoMode::Pipe
        && let Some(prompt) = &request.prompt
    {
        env.insert("JERYU_AGENT_PROMPT".to_string(), prompt.clone());
    }
    let snapshot = lease.frozen_snapshot.as_ref();
    Ok(ResolvedAgentRun {
        source: AgentRunSourceSnapshot::Workcell {
            workcell_id: lease.workcell_id,
            runner_epoch,
            ci_run_id: snapshot.map(|s| s.ci_run_id.clone()),
            failed_run_id: lease.failed_run_id,
            failed_receipt_id: lease.failed_receipt_id,
            failure_log_digest: lease.failure_log_digest,
        },
        repo_root,
        program,
        env,
    })
}

fn select_repo_root(
    lease: &WorkcellLease,
    requested: Option<&Path>,
) -> AgentRunResponseResult<PathBuf> {
    let selected = match requested {
        Some(path) => path.to_path_buf(),
        None => lease.repo_roots.first().cloned().ok_or_else(|| {
            agent_run_path_denied("the workcell has no claimed repo roots to run inside")
        })?,
    };
    let selected = canonical_existing(&selected, "the selected repo root does not exist")?;
    let allowed = lease
        .repo_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| selected == root);
    if !allowed {
        return Err(agent_run_path_denied(
            "the selected repo root is outside the held workcell slice",
        ));
    }
    Ok(selected)
}

fn resolve_program(repo_root: &Path, program: &str) -> AgentRunResponseResult<PathBuf> {
    let candidate = PathBuf::from(program);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        repo_root.join(candidate)
    };
    let candidate = canonical_existing(&candidate, "the requested agent program does not exist")?;
    if !candidate.starts_with(repo_root) {
        return Err(agent_run_path_denied(
            "the requested agent program is outside the selected repo root",
        ));
    }
    Ok(candidate)
}

fn canonical_existing(path: &Path, reason: &'static str) -> AgentRunResponseResult<PathBuf> {
    path.canonicalize()
        .map_err(|_| agent_run_path_denied(reason))
}

fn inject_workcell_env(env: &mut BTreeMap<String, String>, lease: &WorkcellLease) {
    env.insert("JERYU_WORKCELL_ID".to_string(), lease.workcell_id.clone());
    env.insert(
        "JERYU_RUNNER_EPOCH".to_string(),
        lease.runner_epoch.to_string(),
    );
    if let Some(snapshot) = &lease.frozen_snapshot {
        env.insert("JERYU_CI_RUN_ID".to_string(), snapshot.ci_run_id.clone());
        env.insert(
            "JERYU_FAILED_RUN_ID".to_string(),
            snapshot.failed_run_id.clone(),
        );
        env.insert(
            "JERYU_FAILED_RECEIPT_ID".to_string(),
            snapshot.failed_receipt_id.clone(),
        );
        env.insert(
            "JERYU_FAILURE_LOG_DIGEST".to_string(),
            snapshot.failure_log_digest.clone(),
        );
    }
    if let Some(failed_run_id) = &lease.failed_run_id {
        env.entry("JERYU_FAILED_RUN_ID".to_string())
            .or_insert_with(|| failed_run_id.clone());
    }
    if let Some(receipt_id) = &lease.failed_receipt_id {
        env.entry("JERYU_FAILED_RECEIPT_ID".to_string())
            .or_insert_with(|| receipt_id.clone());
    }
    if let Some(digest) = &lease.failure_log_digest {
        env.entry("JERYU_FAILURE_LOG_DIGEST".to_string())
            .or_insert_with(|| digest.clone());
    }
}

impl AgentRunRecord {
    /// Single publish point for one raw TTY event: append it to the bounded ring,
    /// then fan it out to any live SSE subscriber. The ring push happens first so a
    /// subscriber that resyncs after a broadcast overflow always finds the event in
    /// the retained byte history. A send with no live receivers is a harmless drop.
    fn publish_tty(&mut self, event: AgentTtyEvent) {
        self.tty_events.push(event.clone());
        let _ = self.tty_tx.send(event);
    }
}

impl AgentRunStore {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn allocate_id(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        format!("ar-{id:06}")
    }

    fn insert(&self, record: AgentRunRecord) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        inner.runs.insert(record.id.clone(), record);
    }

    /// Record a freshly-launched repo-scoped agent session. The run starts in the
    /// `Running` state on its own unique branch; the `repo` it carries is what the
    /// per-repo agent-runs route filters on, so a session is only ever visible to
    /// the repository that owns it.
    pub(super) fn insert_session(&self, init: SessionRecordInit) {
        self.insert(AgentRunRecord {
            id: init.run_id,
            state: AgentRunState::Running,
            io_mode: AgentRunIoMode::Pty,
            source: AgentRunSourceSnapshot::Repo {
                repo: init.repo.clone(),
            },
            repo_root: init.workspace,
            program: init.program,
            args: init.args,
            events: Vec::new(),
            tty_events: TtyRing::new(),
            tty_tx: new_tty_broadcast(),
            controls: Vec::new(),
            outcome: None,
            error_code: None,
            error_message: None,
            // The session's live control channel: the launch records the sender so
            // the web terminal can steer the PTY agent, and hands the receiver to
            // the driver thread.
            control_tx: init.control_tx,
            repo: Some(init.repo),
            branch: Some(init.branch),
            base_oid: Some(init.base_oid),
            runner: Some(init.runner),
            agent: Some(init.agent),
        });
    }

    /// Record one clear TTY line for a session whose agent binary could not be
    /// resolved, then mark the run finished. The New Session request still returns
    /// a recorded run, and the web terminal degrades gracefully to a single line
    /// that names the missing agent instead of an empty stream.
    pub(super) fn note_agent_unavailable(&self, run_id: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        let program = record.program.clone();
        let agent = record.agent.clone().unwrap_or_else(|| "agent".to_string());
        let line = format!("agent {agent} not available: {program}\r\n");
        let seq = (record.events.len() as u64).saturating_add(1);
        let event = AgentRunEventInput {
            kind: "tty",
            stream: Some("stderr"),
            text: Some(line),
            pid: None,
            used: None,
            limit: None,
            exit_code: None,
            timed_out: false,
            budget_exceeded: false,
        }
        .into_event(seq);
        let tty_event = tty_event_for(record, &event);
        record.events.push(event);
        record.publish_tty(tty_event);
        record.state = AgentRunState::Failed;
        record.control_tx = None;
    }

    /// Record one clear TTY line for a session whose workspace checkout could not be
    /// materialized (the bare clone or branch checkout failed), then mark the run
    /// failed. The New Session request still returns a recorded run and 2xx; the web
    /// terminal degrades to a single line naming the reason rather than launching an
    /// agent against an empty, code-less workspace.
    pub(super) fn note_session_checkout_failed(&self, run_id: &str, reason: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        let line = format!("session workspace checkout failed: {reason}\r\n");
        let seq = (record.events.len() as u64).saturating_add(1);
        let event = AgentRunEventInput {
            kind: "tty",
            stream: Some("stderr"),
            text: Some(line),
            pid: None,
            used: None,
            limit: None,
            exit_code: None,
            timed_out: false,
            budget_exceeded: false,
        }
        .into_event(seq);
        let tty_event = tty_event_for(record, &event);
        record.events.push(event);
        record.publish_tty(tty_event);
        record.state = AgentRunState::Failed;
        record.control_tx = None;
    }

    /// Live agent-run rows for ONE repository. Filters strictly on the run's owning
    /// `repo`, so runs that belong to a different repository (or to a workcell, with
    /// no repo) are never returned here — the data-isolation invariant the
    /// per-repo route depends on.
    pub(super) fn rows_for_repo(&self, repo_full_name: &str) -> Vec<RepoAgentRunRow> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .values()
            .filter(|record| {
                record.repo.as_deref() == Some(repo_full_name)
                    && record.agent.as_deref() != Some("shell")
            })
            .map(|record| {
                let mut row = RepoAgentRunRow::from_record(record);
                row.shell_run_id = inner.shell_companions.get(&record.id).cloned();
                row
            })
            .collect()
    }

    /// Register a companion shell for a given agent run.
    pub(super) fn register_shell_companion(&self, agent_run_id: &str, shell_run_id: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        inner
            .shell_companions
            .insert(agent_run_id.to_string(), shell_run_id.to_string());
    }

    /// The branch + base oid + state needed to mediate a publish for one run.
    pub(super) fn publish_info(&self, run_id: &str) -> Option<SessionPublishInfo> {
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        Some(SessionPublishInfo {
            repo: record.repo.clone(),
            branch: record.branch.clone(),
            base_oid: record.base_oid.clone(),
            state: record.state,
        })
    }

    fn status(&self, run_id: &str) -> Option<AgentRunStatusResponse> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .get(run_id)
            .and_then(|record| status_from_record(run_id, record))
    }

    pub(super) fn list(&self) -> Vec<AgentRunStatusResponse> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .iter()
            .filter_map(|(run_id, record)| status_from_record(run_id, record))
            .collect()
    }

    pub(super) fn list_json(&self) -> Vec<Value> {
        self.list()
            .into_iter()
            .filter_map(|status| serde_json::to_value(status).ok())
            .collect()
    }

    fn events(&self, run_id: &str, query: AgentRunEventsQuery) -> Option<AgentRunEventsResponse> {
        let after_seq = query.after_seq.unwrap_or(0);
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000);
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        let all_events: Vec<AgentRunEvent> = record
            .events
            .iter()
            .filter(|event| event.seq > after_seq)
            .cloned()
            .collect();
        let has_more = all_events.len() > limit;
        let events: Vec<AgentRunEvent> = all_events.into_iter().take(limit).collect();
        let next_after_seq = events.last().map(|event| event.seq).unwrap_or(after_seq);
        let tty_events = record
            .tty_events
            .iter()
            .filter(|event| event.seq > after_seq && event.seq <= next_after_seq)
            .cloned()
            .collect::<Vec<_>>();
        Some(AgentRunEventsResponse {
            agent_run_id: record.id.clone(),
            after_seq,
            next_after_seq,
            limit,
            has_more,
            events,
            tty_events,
        })
    }

    fn control_sender(&self, run_id: &str) -> Option<Sender<AgentControl>> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .get(run_id)
            .and_then(|record| record.control_tx.clone())
    }

    fn record(&self, run_id: &str) -> Option<AgentRunRecord> {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner.runs.get(run_id).cloned()
    }

    pub(super) fn mark_exported(&self, run_id: &str) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        if let Some(record) = inner.runs.get_mut(run_id) {
            record.state = AgentRunState::Exported;
        }
    }

    fn send_control(
        &self,
        run_id: &str,
        command: AgentControlCommand,
    ) -> AgentRunResponseResult<AgentRunControlResponse> {
        let (tx, control, command_name, seq) = {
            let mut inner = self.inner.lock().expect("agent run store mutex");
            let record = inner
                .runs
                .get_mut(run_id)
                .ok_or_else(|| Box::new(agent_run_not_found(run_id)))?;
            if record.state != AgentRunState::Running {
                return Err(boxed_agent_run_typed_error(
                    StatusCode::CONFLICT,
                    "agent_run_finished",
                    "send control to an agent run",
                    "the agent run is already finished",
                    &[
                        "reload the run status before sending more control",
                        "start a new agent run for additional repair work",
                    ],
                    "start a fresh run, then send control while it is running",
                ));
            }
            if record.io_mode != AgentRunIoMode::Pty {
                return Err(boxed_agent_run_typed_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "agent_run_control_unsupported",
                    "send control to an agent run",
                    "the selected io_mode does not support live control",
                    &[
                        "start the run with io_mode pty",
                        "use pipe mode only for deterministic non-interactive commands",
                    ],
                    "rerun the agent with io_mode pty before sending control",
                ));
            }
            let Some(tx) = record.control_tx.clone() else {
                return Err(boxed_agent_run_typed_error(
                    StatusCode::CONFLICT,
                    "agent_run_control_unavailable",
                    "send control to an agent run",
                    "the live control channel is no longer available",
                    &[
                        "reload the run status before sending more control",
                        "check whether the driver has already exited",
                    ],
                    "retry only while the run is still marked running",
                ));
            };
            let control = map_control(&command);
            let command_name = command_name(&command).to_string();
            let seq = (record.controls.len() as u64).saturating_add(1);
            record.controls.push(AgentRunControlRecord {
                seq,
                command: command_name.clone(),
            });
            (tx, control, command_name, seq)
        };
        tx.send(control).map_err(|_| {
            boxed_agent_run_typed_error(
                StatusCode::CONFLICT,
                "agent_run_control_closed",
                "send control to an agent run",
                "the live driver stopped before the control command was delivered",
                &[
                    "reload the run status before sending more control",
                    "start a new run if more repair work is required",
                ],
                "send controls only while the status endpoint reports running",
            )
        })?;
        Ok(AgentRunControlResponse {
            agent_run_id: run_id.to_string(),
            accepted: true,
            control_seq: seq,
            command: command_name,
        })
    }

    fn append_event(&self, run_id: &str, event: AgentRunEventInput) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        let seq = (record.events.len() as u64).saturating_add(1);
        let event = event.into_event(seq);
        let tty_event = tty_event_for(record, &event);
        record.events.push(event);
        record.publish_tty(tty_event);
    }

    /// Cursor-pull tail of one run's raw TTY events. Returns every retained event
    /// with `seq > after_seq` (raw `bytes_b64` intact), capped at `limit`. When the
    /// cursor fell behind the drop-oldest ring `lagged` is true and the events come
    /// from the oldest retained `seq` so the caller (jpmc) can resync. `next_after_seq`
    /// is the highest returned `seq`, or the input cursor when nothing is newer.
    fn tail_tty(
        &self,
        run_id: &str,
        after_seq: u64,
        limit: Option<usize>,
    ) -> Option<AgentRunTailResponse> {
        let limit = limit.unwrap_or(TTY_RING_CAP).clamp(1, TTY_RING_CAP);
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        let lagged = record.tty_events.lagged(after_seq);
        let events = record.tty_events.tail(after_seq, limit);
        let next_after_seq = events.last().map_or(after_seq, |event| event.seq);
        Some(AgentRunTailResponse {
            agent_run_id: record.id.clone(),
            after_seq,
            next_after_seq,
            oldest_retained_seq: record.tty_events.oldest_retained_seq,
            lagged,
            tty_topic: TTY_TOPIC.to_string(),
            events,
        })
    }

    /// Atomically open a live SSE subscription for one run: under a single lock it
    /// subscribes to the run's broadcast AND snapshots the retained ring past
    /// `after_seq`. Subscribing before releasing the lock guarantees no event slips
    /// between the replay snapshot and the live feed (any later publish also takes
    /// the lock), so the caller only needs to drop live events whose `seq` is not
    /// past the replay's `next_after_seq`. Returns `None` for an unknown run, which
    /// is how the SSE edge denies an unknown or non-member scope.
    fn tty_stream_start(
        &self,
        run_id: &str,
        after_seq: u64,
    ) -> Option<(broadcast::Receiver<AgentTtyEvent>, AgentRunTailResponse)> {
        let inner = self.inner.lock().expect("agent run store mutex");
        let record = inner.runs.get(run_id)?;
        let receiver = record.tty_tx.subscribe();
        let lagged = record.tty_events.lagged(after_seq);
        let events = record.tty_events.tail(after_seq, TTY_RING_CAP);
        let next_after_seq = events.last().map_or(after_seq, |event| event.seq);
        let response = AgentRunTailResponse {
            agent_run_id: record.id.clone(),
            after_seq,
            next_after_seq,
            oldest_retained_seq: record.tty_events.oldest_retained_seq,
            lagged,
            tty_topic: TTY_TOPIC.to_string(),
            events,
        };
        Some((receiver, response))
    }

    /// The oldest raw TTY `seq` still retained for one run (0 when empty or unknown).
    /// A live SSE stream reads this when its broadcast overflows so the `resync`
    /// marker tells a lagged subscriber the floor to re-pull the ring from.
    fn oldest_retained_tty_seq(&self, run_id: &str) -> u64 {
        let inner = self.inner.lock().expect("agent run store mutex");
        inner
            .runs
            .get(run_id)
            .map_or(0, |record| record.tty_events.oldest_retained_seq)
    }

    fn complete(&self, run_id: &str, result: Result<AgentRunResult, DriverError>) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        let Some(record) = inner.runs.get_mut(run_id) else {
            return;
        };
        record.control_tx = None;
        match result {
            Ok(result) => {
                let outcome = AgentRunOutcome::from_result(result);
                record.state = if outcome.succeeded {
                    AgentRunState::Succeeded
                } else {
                    AgentRunState::Failed
                };
                record.outcome = Some(outcome);
            }
            Err(err) => {
                let (code, message) = driver_error_parts(err);
                record.state = AgentRunState::Failed;
                record.error_code = Some(code.to_string());
                record.error_message = Some(message);
            }
        }
    }
}

#[cfg(test)]
impl AgentRunStore {
    /// Seed a minimal repo-scoped run for tail/ring coverage with a chosen ring
    /// cap so eviction can be forced without pushing the whole production bound.
    pub(super) fn seed_test_run(&self, run_id: &str, ring_cap: usize) {
        self.insert(AgentRunRecord {
            id: run_id.to_string(),
            state: AgentRunState::Running,
            io_mode: AgentRunIoMode::Pty,
            source: AgentRunSourceSnapshot::Repo {
                repo: "owner/repo".to_string(),
            },
            repo_root: PathBuf::from("/tmp/agent-run"),
            program: "agent".to_string(),
            args: Vec::new(),
            events: Vec::new(),
            tty_events: TtyRing::with_cap(ring_cap),
            tty_tx: new_tty_broadcast(),
            controls: Vec::new(),
            outcome: None,
            error_code: None,
            error_message: None,
            control_tx: None,
            repo: Some("owner/repo".to_string()),
            branch: Some("sessions/test".to_string()),
            base_oid: Some("oid-test".to_string()),
            runner: None,
            agent: None,
        });
    }

    /// Publish one prebuilt raw TTY event through the same ring-plus-broadcast path
    /// `append_event` uses, so a test can drive both the cursor-pull replay and the
    /// live SSE fan-out from one helper.
    pub(super) fn push_test_tty(&self, run_id: &str, event: AgentTtyEvent) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        if let Some(record) = inner.runs.get_mut(run_id) {
            record.publish_tty(event);
        }
    }
}

/// Live fan-out depth of the per-run raw TTY broadcast, exposed so a route test can
/// overflow a slow subscriber by exactly enough to force the resync path.
#[cfg(test)]
pub(super) fn tty_broadcast_capacity() -> usize {
    TTY_BROADCAST_CAP
}

/// Build a raw (non-text) TTY event whose payload is the base64 of `bytes`, so a
/// test can prove a non-UTF8 byte sequence survives the ring and tail byte-for-byte.
#[cfg(test)]
pub(super) fn test_raw_tty_event(run_id: &str, seq: u64, bytes: &[u8]) -> AgentTtyEvent {
    use base64::Engine;
    let key = AgentRunStreamKey {
        repo: Some("owner/repo".to_string()),
        workcell_id: "owner/repo".to_string(),
        agent_run_id: run_id.to_string(),
        agent: "agent".to_string(),
        model: "local".to_string(),
    };
    let mut event = AgentTtyEvent::text(seq, 0, &key, AgentOutputStream::Pty, String::new());
    event.text = None;
    event.bytes_b64 = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
    event
}

fn status_from_record(run_id: &str, record: &AgentRunRecord) -> Option<AgentRunStatusResponse> {
    if record.id != run_id {
        return None;
    }
    Some(AgentRunStatusResponse {
        agent_run_id: record.id.clone(),
        state: record.state,
        io_mode: record.io_mode,
        source: record.source.clone(),
        repo_root: record.repo_root.clone(),
        program: record.program.clone(),
        args: record.args.clone(),
        events_url: format!("/api/v1/agent-runs/{run_id}/events"),
        control_url: format!("/api/v1/agent-runs/{run_id}/control"),
        export_pr_url: format!("/api/v1/agent-runs/{run_id}/export_pr"),
        ws_scope: format!("agent_run.{run_id}"),
        tty_topic: TTY_TOPIC.to_string(),
        control_topic: CONTROL_TOPIC.to_string(),
        events: record.events.clone(),
        tty_events: record.tty_events.snapshot(),
        controls: record.controls.clone(),
        outcome: record.outcome.clone(),
        error_code: record.error_code.clone(),
        error_message: record.error_message.clone(),
    })
}

fn tty_event_for(record: &AgentRunRecord, event: &AgentRunEvent) -> AgentTtyEvent {
    let key = stream_key_for(record);
    let stream = match event.stream.as_deref() {
        Some("stdout") => AgentOutputStream::Stdout,
        Some("stderr") => AgentOutputStream::Stderr,
        Some("pty") => AgentOutputStream::Pty,
        _ => AgentOutputStream::Event,
    };
    let mut tty = if event.kind == "finished" {
        AgentTtyEvent::finished(
            event.seq,
            epoch_millis(),
            &key,
            event.exit_code,
            record
                .outcome
                .as_ref()
                .map(|outcome| outcome.enforcement_level.clone())
                .unwrap_or_else(|| "pending".to_string()),
        )
    } else {
        AgentTtyEvent::text(
            event.seq,
            epoch_millis(),
            &key,
            stream,
            event.text.clone().unwrap_or_else(|| event.kind.clone()),
        )
    };
    if let Some(limit) = event.limit {
        tty.budget = Some(AgentEventBudget {
            wall_secs: 0,
            output_bytes: limit as u64,
            used_output_bytes: event.used.unwrap_or(0) as u64,
        });
    }
    tty
}

fn stream_key_for(record: &AgentRunRecord) -> AgentRunStreamKey {
    let workcell_id = match &record.source {
        AgentRunSourceSnapshot::Workcell { workcell_id, .. } => workcell_id.clone(),
        AgentRunSourceSnapshot::Repo { repo } => repo.clone(),
        AgentRunSourceSnapshot::LocalPath { local_path } => {
            local_path.to_string_lossy().to_string()
        }
        AgentRunSourceSnapshot::Scratch { name } => {
            name.clone().unwrap_or_else(|| "scratch".to_string())
        }
    };
    let repo = match &record.source {
        AgentRunSourceSnapshot::Repo { repo } => Some(repo.clone()),
        AgentRunSourceSnapshot::Workcell { .. }
        | AgentRunSourceSnapshot::LocalPath { .. }
        | AgentRunSourceSnapshot::Scratch { .. } => None,
    };
    AgentRunStreamKey {
        repo,
        workcell_id,
        agent_run_id: record.id.clone(),
        agent: agent_label(&record.program),
        model: "local".to_string(),
    }
}

fn agent_label(program: &str) -> String {
    Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("agent")
        .to_string()
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn export_workcell_agent_run(
    state: &Arc<WebState>,
    agent_run_id: &str,
    request: AgentRunExportPrRequest,
    origin_base_url: &str,
) -> AgentRunResponseResult<AgentRunExportPrResponse> {
    let record = state
        .agent_runs
        .record(agent_run_id)
        .ok_or_else(|| Box::new(agent_run_not_found(agent_run_id)))?;
    if record.state == AgentRunState::Running {
        return Err(boxed_agent_run_typed_error(
            StatusCode::CONFLICT,
            "agent_run_not_finished",
            "export an agent run into a pull request",
            "the run must finish before export can freeze the diff",
            &[
                "wait for the run to exit before exporting",
                "reload /api/v1/agent-runs/{id} and retry with a terminal run",
            ],
            "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
        ));
    }
    let (workcell_id, runner_epoch) = match &record.source {
        AgentRunSourceSnapshot::Workcell {
            workcell_id,
            runner_epoch,
            ..
        } => (workcell_id.clone(), *runner_epoch),
        _ => {
            return Err(boxed_agent_run_typed_error(
                StatusCode::FAILED_DEPENDENCY,
                "agent_run_export_source_unavailable",
                "export an agent run into a pull request",
                "only workcell-backed agent runs can be exported by the current route",
                &[
                    "start the run from a held or repairing workcell",
                    "wire repository and scratch source materialization before exporting those sources",
                ],
                "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
            ));
        }
    };

    let (lease, branch) = {
        let mut manager = manager(state);
        let branch_suffix = request
            .branch_suffix
            .clone()
            .unwrap_or_else(|| format!("agent-run-{agent_run_id}"));
        let branch = match manager.export_repair_branch(&workcell_id, runner_epoch, branch_suffix) {
            Ok(branch) => branch,
            Err(err) => return Err(Box::new(super::workcells_support::workcell_error(err))),
        };
        let Some(lease) = manager.workcell(&workcell_id).cloned() else {
            return Err(Box::new(super::workcells_support::workcell_not_found(
                &workcell_id,
            )));
        };
        (lease, branch)
    };

    let target_branch = request
        .target_branch
        .clone()
        .or_else(|| lease.startup_main_ref.clone())
        .map(normalize_pr_base)
        .unwrap_or_else(|| "main".to_string());
    let snapshot = lease.frozen_snapshot.as_ref();
    let head_sha = snapshot
        .map(|snapshot| snapshot.head_sha.clone())
        .or_else(|| lease.startup_head_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let base_sha = snapshot
        .map(|snapshot| snapshot.base_sha.clone())
        .or_else(|| lease.startup_base_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let allowed_prefixes = derive_allowed_prefixes(&lease.allowed_paths, &lease.workspace_root);
    let bare_repo = match state
        .repo_manager
        .resolve_parts(&request.owner, &request.repo)
    {
        Ok(repository) => repository.path,
        Err(err) => return Err(Box::new(forge_error(ForgeError::Storage(err.to_string())))),
    };
    let git_bin = state.repo_manager.config().git_bin.clone();
    let changed_files = match jeryu_codegraph::enforce_export_slice(
        &base_sha,
        &head_sha,
        &git_bin,
        &bare_repo,
        &allowed_prefixes,
    ) {
        Ok(files) => files,
        Err(denied) => {
            let message = match denied.git_error {
                Some(git_error) => {
                    format!("the export slice gate could not verify the diff: {git_error}")
                }
                None => format!(
                    "the export changed files outside the agent-run slice: {}",
                    denied.out_of_slice_paths.join(", ")
                ),
            };
            return Err(Box::new(typed_error(TypedError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "agent_run_export_slice_denied",
                purpose: "export an agent run into a pull request",
                reason: &message,
                common_fixes: &[
                    "restrict the agent edits to files inside the workcell's allowed paths",
                    "reclaim the workcell with a lease that covers the changed files",
                ],
                docs_url: "docs/testing.md#workcells",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
                message: &message,
            })));
        }
    };
    let pr = match state.github.core().create_pull_request(
        &request.owner,
        &request.repo,
        &request.author,
        CreatePullRequestRequest {
            title: request.title,
            body: request.body,
            head: branch.clone(),
            base: target_branch.clone(),
            head_sha: Some(head_sha),
            base_sha: Some(base_sha),
            source_repository: Some(format!("{}/{}", request.owner, request.repo)),
            draft: false,
            commits: Vec::new(),
            changed_files,
        },
    ) {
        Ok(pr) => pr,
        Err(err) => return Err(Box::new(forge_error(err))),
    };
    crate::ci_bridge::seed_pull_request_head(
        state.github.core(),
        state.repo_manager.as_ref(),
        &request.owner,
        &request.repo,
        &format!("refs/heads/{}", pr.head.ref_name),
        &pr.head.sha,
        origin_base_url,
    );
    state.agent_runs.mark_exported(agent_run_id);
    Ok(AgentRunExportPrResponse {
        agent_run_id: agent_run_id.to_string(),
        branch,
        target_branch,
        pull_request_number: pr.number,
        url: format!("/{}/{}/pull/{}", pr.owner, pr.repo, pr.number),
    })
}

pub(super) fn origin_base_url(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.trim().is_empty())
        .map(|host| format!("http://{host}"))
        .unwrap_or_else(crate::ci_bridge::default_origin_base_url)
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

#[cfg(test)]
mod tail_tests {
    use base64::Engine;

    use super::{AgentRunEventInput, AgentRunStore, test_raw_tty_event};

    fn decode(event: &super::AgentTtyEvent) -> Vec<u8> {
        let encoded = event.bytes_b64.as_deref().expect("raw bytes payload");
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("base64 decode")
    }

    #[test]
    fn tail_returns_events_past_cursor_with_raw_bytes_intact() {
        let store = AgentRunStore::new();
        store.seed_test_run("ar-tail", 16);
        // A deliberately non-UTF8 byte sequence to prove byte-for-byte fidelity.
        let raw_two = [0xff_u8, 0x00, 0xfe, b'h', b'i', 0x80];
        store.push_test_tty("ar-tail", test_raw_tty_event("ar-tail", 1, b"one"));
        store.push_test_tty("ar-tail", test_raw_tty_event("ar-tail", 2, &raw_two));
        store.push_test_tty("ar-tail", test_raw_tty_event("ar-tail", 3, b"three"));

        let tail = store.tail_tty("ar-tail", 1, None).expect("tail result");
        assert!(!tail.lagged);
        assert_eq!(tail.after_seq, 1);
        assert_eq!(tail.next_after_seq, 3);
        let seqs: Vec<u64> = tail.events.iter().map(|event| event.seq).collect();
        assert_eq!(seqs, vec![2, 3], "only events strictly after the cursor");
        assert_eq!(
            decode(&tail.events[0]),
            raw_two,
            "non-UTF8 bytes round-trip byte-identical through bytes_b64"
        );
    }

    #[test]
    fn tail_from_zero_advances_then_returns_only_new_events() {
        let store = AgentRunStore::new();
        store.seed_test_run("ar-cursor", 16);
        store.push_test_tty("ar-cursor", test_raw_tty_event("ar-cursor", 1, b"a"));
        store.push_test_tty("ar-cursor", test_raw_tty_event("ar-cursor", 2, b"b"));

        let first = store.tail_tty("ar-cursor", 0, None).expect("first tail");
        assert!(!first.lagged);
        assert_eq!(
            first.events.len(),
            2,
            "after_seq=0 starts from the beginning"
        );
        assert_eq!(first.next_after_seq, 2);

        let empty = store
            .tail_tty("ar-cursor", first.next_after_seq, None)
            .expect("empty tail");
        assert!(empty.events.is_empty(), "no events newer than the cursor");
        assert_eq!(
            empty.next_after_seq, 2,
            "next cursor holds when nothing is newer"
        );

        store.push_test_tty("ar-cursor", test_raw_tty_event("ar-cursor", 3, b"c"));
        let resumed = store
            .tail_tty("ar-cursor", empty.next_after_seq, None)
            .expect("resumed tail");
        let seqs: Vec<u64> = resumed.events.iter().map(|event| event.seq).collect();
        assert_eq!(
            seqs,
            vec![3],
            "second tail returns only the freshly pushed event"
        );
    }

    #[test]
    fn ring_eviction_marks_lagged_from_evicted_cursor_only() {
        let store = AgentRunStore::new();
        store.seed_test_run("ar-ring", 4);
        // Push six events into a four-slot ring: seq 1 and 2 roll off the front.
        for seq in 1..=6 {
            store.push_test_tty(
                "ar-ring",
                test_raw_tty_event("ar-ring", seq, format!("chunk-{seq}").as_bytes()),
            );
        }

        let evicted = store.tail_tty("ar-ring", 1, None).expect("evicted tail");
        assert!(evicted.lagged, "a cursor behind the ring is flagged lagged");
        assert_eq!(
            evicted.oldest_retained_seq, 3,
            "oldest retained seq after eviction"
        );
        let seqs: Vec<u64> = evicted.events.iter().map(|event| event.seq).collect();
        assert_eq!(
            seqs,
            vec![3, 4, 5, 6],
            "a lagged tail resyncs from the oldest retained event"
        );

        let live = store.tail_tty("ar-ring", 4, None).expect("live tail");
        assert!(!live.lagged, "a cursor inside the ring is not lagged");
        let live_seqs: Vec<u64> = live.events.iter().map(|event| event.seq).collect();
        assert_eq!(live_seqs, vec![5, 6]);

        // A cursor exactly on the last evicted seq is contiguous, not lagged.
        let boundary = store.tail_tty("ar-ring", 2, None).expect("boundary tail");
        assert!(!boundary.lagged);
    }

    #[test]
    fn append_event_still_feeds_the_ws_snapshot_feed() {
        let store = AgentRunStore::new();
        store.seed_test_run("ar-ws", 16);
        store.append_event(
            "ar-ws",
            AgentRunEventInput {
                kind: "tty",
                stream: Some("stdout"),
                text: Some("ws-visible-line".to_string()),
                pid: None,
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
        );

        // The WS tty stream renders from status(); the appended event must show up.
        let status = store.status("ar-ws").expect("status snapshot");
        assert!(
            status
                .tty_events
                .iter()
                .any(|event| event.text.as_deref() == Some("ws-visible-line")),
            "append_event remains the publish point feeding WS subscribers"
        );
        // The same event is tailable as a raw cursor-pull payload.
        let tail = store.tail_tty("ar-ws", 0, None).expect("tail snapshot");
        assert_eq!(tail.events.len(), 1);
        assert!(!tail.lagged);
    }

    #[test]
    fn append_event_fans_out_to_a_live_broadcast_subscriber() {
        let store = AgentRunStore::new();
        store.seed_test_run("ar-fanout", 16);
        // Open a live subscription first, then publish through the one publish point.
        let (mut receiver, replay) = store.tty_stream_start("ar-fanout", 0).expect("subscribe");
        assert!(
            replay.events.is_empty(),
            "no buffered events before publish"
        );

        store.append_event(
            "ar-fanout",
            AgentRunEventInput {
                kind: "tty",
                stream: Some("stdout"),
                text: Some("fanout-line".to_string()),
                pid: None,
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
        );

        let live = receiver
            .try_recv()
            .expect("append_event fans the event out to the live broadcast");
        assert_eq!(live.text.as_deref(), Some("fanout-line"));
        // The same event still sits in the ring for a resyncing cursor-pull tailer.
        let tail = store.tail_tty("ar-fanout", 0, None).expect("tail snapshot");
        assert_eq!(tail.events.len(), 1);
        assert_eq!(tail.events[0].text.as_deref(), Some("fanout-line"));
    }
}
