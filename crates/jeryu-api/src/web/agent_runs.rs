use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
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
use serde_json::Value;

use super::WebState;
use super::surface::serialize_payload;
use super::workcells_support::{TypedError, forge_error, manager, typed_error};

const AGENT_RUN_DOCS: &str = "docs/workcell.md#agent-run-control-surface";
const AGENT_RUN_RERUN: &str = "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs";
type AgentRunResponseResult<T> = Result<T, Box<AxumResponse>>;

#[derive(Clone, Default)]
pub(crate) struct AgentRunStore {
    inner: Arc<Mutex<AgentRunStoreInner>>,
    next_id: Arc<AtomicU64>,
}

#[derive(Default)]
struct AgentRunStoreInner {
    runs: BTreeMap<String, AgentRunRecord>,
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
    tty_events: Vec<AgentTtyEvent>,
    controls: Vec<AgentRunControlRecord>,
    outcome: Option<AgentRunOutcome>,
    error_code: Option<String>,
    error_message: Option<String>,
    control_tx: Option<Sender<AgentControl>>,
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
        tty_events: Vec::new(),
        controls: Vec::new(),
        outcome: None,
        error_code: None,
        error_message: None,
        control_tx: control,
    });

    if let Some(prompt) = request.prompt.clone()
        && request.io_mode == AgentRunIoMode::Pty
    {
        let _ = state
            .agent_runs
            .control_sender(&agent_run_id)
            .and_then(|tx| tx.send(AgentControl::InjectPrompt(prompt)).ok());
    }

    let store = state.agent_runs.clone();
    let run_id_for_thread = agent_run_id.clone();
    let run_root = resolved.repo_root;
    let mode = request.io_mode;
    let require_cgroup = request.require_cgroup();
    std::thread::spawn(move || {
        let sink = RecordingSink {
            store: store.clone(),
            run_id: run_id_for_thread.clone(),
        };
        let result = match mode {
            AgentRunIoMode::Pty => PtyAgentDriver::new(timeout, output_budget)
                .with_require_cgroup(require_cgroup)
                .run(&run_root, &spec, &sink, &control_rx),
            AgentRunIoMode::Pipe => AgentDriver::new(timeout, output_budget)
                .with_require_cgroup(require_cgroup)
                .run(&run_root, &spec, &sink),
        };
        store.complete(&run_id_for_thread, result);
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

pub(super) async fn export_pr(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: AgentRunExportPrRequest =
        match parse_agent_body(&body, "export an agent run into a pull request") {
            Ok(request) => request,
            Err(response) => return *response,
        };
    match export_workcell_agent_run(&state, &agent_run_id, request) {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(response) => *response,
    }
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

pub(super) fn mcp_export_pr(state: &Arc<WebState>, args: Value) -> Result<Value, String> {
    let run_id = required_run_id(&args)?;
    let request: AgentRunExportPrRequest =
        serde_json::from_value(args).map_err(|err| err.to_string())?;
    let response = export_workcell_agent_run(state, &run_id, request).map_err(|_| {
        "agent_work.export_pr failed; use REST for typed repair details".to_string()
    })?;
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

impl AgentRunStore {
    pub(super) fn new() -> Self {
        Self::default()
    }

    fn allocate_id(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        format!("ar-{id:06}")
    }

    fn insert(&self, record: AgentRunRecord) {
        let mut inner = self.inner.lock().expect("agent run store mutex");
        inner.runs.insert(record.id.clone(), record);
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
            .collect();
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

    fn mark_exported(&self, run_id: &str) {
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
        record.tty_events.push(tty_event);
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
        tty_events: record.tty_events.clone(),
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
    state.agent_runs.mark_exported(agent_run_id);
    Ok(AgentRunExportPrResponse {
        agent_run_id: agent_run_id.to_string(),
        branch,
        target_branch,
        pull_request_number: pr.number,
        url: format!("/{}/{}/pull/{}", pr.owner, pr.repo, pr.number),
    })
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
