use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_agentbridge::driver::{
    AgentDriver, AgentEvent, AgentEventSink, AgentRunResult, CommandSpec, DriverError,
};
use jeryu_runnerd::{WorkcellLease, WorkcellState};
use serde::{Deserialize, Serialize};

use crate::web::WebState;
use crate::web::workcells_support::{
    TypedError, default_true, manager, parse_json_body, typed_error, workcell_not_found,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentWorkcellRunRequest {
    pub workcell_id: String,
    pub runner_epoch: u64,
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub output_budget_bytes: Option<usize>,
    #[serde(default = "default_true")]
    pub require_cgroup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentWorkcellRunEvent {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentWorkcellRunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub budget_exceeded: bool,
    pub captured_bytes: usize,
    pub enforcement_level: String,
    pub elapsed_ms: u64,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentWorkcellRunResponse {
    pub workcell_id: String,
    pub runner_epoch: u64,
    pub repo_root: PathBuf,
    pub events: Vec<AgentWorkcellRunEvent>,
    pub outcome: AgentWorkcellRunOutcome,
}

pub(in crate::web) async fn run_agent(
    State(state): State<Arc<WebState>>,
    AxumPath(workcell_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: AgentWorkcellRunRequest = match parse_json_body(
        &body,
        "run an agent inside a workcell repo slice",
        "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    if request.workcell_id != workcell_id {
        return typed_error(TypedError {
            status: StatusCode::BAD_REQUEST,
            code: "workcell_id_mismatch",
            purpose: "run an agent inside a workcell repo slice",
            reason: "request path and body disagreed on the workcell id",
            common_fixes: &[
                "send the same workcell id in the path and request body",
                "reload the workcell status before retrying the run",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
            message: "the request body did not match the selected workcell",
        });
    }

    let lease = match manager(&state).workcell(&workcell_id).cloned() {
        Some(lease) => lease,
        None => return workcell_not_found(&workcell_id),
    };
    if lease.runner_epoch != request.runner_epoch {
        return typed_error(TypedError {
            status: StatusCode::CONFLICT,
            code: "workcell_epoch_fenced",
            purpose: "run an agent inside a workcell repo slice",
            reason: "request runner_epoch did not match the active workcell epoch",
            common_fixes: &[
                "reload workcell status and retry with the active runner_epoch",
                "release the prior workcell before starting a new run",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
            message: "the workcell run request used a previous runner epoch",
        });
    }
    if !matches!(
        lease.state,
        WorkcellState::Claimed | WorkcellState::Held | WorkcellState::Repairing
    ) {
        return typed_error(TypedError {
            status: StatusCode::CONFLICT,
            code: "workcell_claim_denied",
            purpose: "run an agent inside a workcell repo slice",
            reason: "the workcell is not claimed, held, or repairing",
            common_fixes: &[
                "claim a ready workcell before running an agent",
                "refresh the workcell status before retrying",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
            message: "the selected workcell is not active",
        });
    }

    let run_root = match selected_run_root(&lease, request.repo_root.as_deref()) {
        Ok(run_root) => run_root,
        Err(response) => return *response,
    };
    let program = match resolve_program_in_run_root(&run_root, &request.program) {
        Ok(program) => program,
        Err(response) => return *response,
    };
    let timeout = Duration::from_millis(request.timeout_ms.unwrap_or(30_000).clamp(1, 300_000));
    let output_budget = request
        .output_budget_bytes
        .unwrap_or(jeryu_agentbridge::driver::DEFAULT_OUTPUT_BUDGET_BYTES)
        .clamp(1, 1024 * 1024);
    let driver =
        AgentDriver::new(timeout, output_budget).with_require_cgroup(request.require_cgroup);
    let spec = CommandSpec {
        program: program.to_string_lossy().to_string(),
        args: request.args,
        env: request.env,
    };
    let run_root_for_task = run_root.clone();
    let task = tokio::task::spawn_blocking(move || {
        let sink = SerializingAgentSink::default();
        let result = driver.run(&run_root_for_task, &spec, &sink);
        (result, sink.events())
    });
    let (result, events) = match task.await {
        Ok(outcome) => outcome,
        Err(err) => {
            let message = format!("agent run task failed: {err}");
            return typed_error(TypedError {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "workcell_run_join_failed",
                purpose: "run an agent inside a workcell repo slice",
                reason: "the blocking agent run task failed to join",
                common_fixes: &[
                    "inspect jeryu-api logs for a panic in the workcell run path",
                    "rerun the focused workcell_run_agent proof lane",
                ],
                docs_url: "docs/testing.md#workcells",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
                message: &message,
            });
        }
    };
    let result = match result {
        Ok(result) => result,
        Err(err) => return driver_error_response(err),
    };

    Json(AgentWorkcellRunResponse {
        workcell_id,
        runner_epoch: request.runner_epoch,
        repo_root: run_root,
        events,
        outcome: AgentWorkcellRunOutcome::from(result),
    })
    .into_response()
}

#[derive(Default)]
struct SerializingAgentSink {
    events: std::sync::Mutex<Vec<AgentWorkcellRunEvent>>,
}

impl SerializingAgentSink {
    fn events(&self) -> Vec<AgentWorkcellRunEvent> {
        self.events.lock().expect("agent sink mutex").clone()
    }
}

impl AgentEventSink for SerializingAgentSink {
    fn emit(&self, ev: AgentEvent) {
        self.events
            .lock()
            .expect("agent sink mutex")
            .push(AgentWorkcellRunEvent::from(ev));
    }
}

impl From<AgentEvent> for AgentWorkcellRunEvent {
    fn from(value: AgentEvent) -> Self {
        match value {
            AgentEvent::Started { pid } => Self {
                kind: "started".to_string(),
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
                kind: "line".to_string(),
                stream: Some("stdout".to_string()),
                text: Some(text),
                pid: None,
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Stderr(text) => Self {
                kind: "line".to_string(),
                stream: Some("stderr".to_string()),
                text: Some(text),
                pid: None,
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Budget { used, limit } => Self {
                kind: "budget".to_string(),
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
                kind: "finished".to_string(),
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

impl From<AgentRunResult> for AgentWorkcellRunOutcome {
    fn from(value: AgentRunResult) -> Self {
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

fn selected_run_root(
    lease: &WorkcellLease,
    requested: Option<&Path>,
) -> Result<PathBuf, Box<AxumResponse>> {
    let selected = match requested {
        Some(path) => path.to_path_buf(),
        None => match lease.repo_roots.first() {
            Some(path) => path.clone(),
            None => {
                return Err(run_path_denied(
                    "the workcell has no claimed repo roots to run inside",
                ));
            }
        },
    };
    let selected = canonical_existing(&selected, "the selected repo root does not exist")?;
    let allowed = lease
        .repo_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| selected == root);
    if !allowed {
        return Err(run_path_denied(
            "the selected repo root is outside the claimed workcell slice",
        ));
    }
    Ok(selected)
}

fn resolve_program_in_run_root(
    run_root: &Path,
    program: &str,
) -> Result<PathBuf, Box<AxumResponse>> {
    let candidate = PathBuf::from(program);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        run_root.join(candidate)
    };
    let candidate = canonical_existing(&candidate, "the requested program does not exist")?;
    if !candidate.starts_with(run_root) {
        return Err(run_path_denied(
            "the requested program is outside the selected repo root",
        ));
    }
    Ok(candidate)
}

fn canonical_existing(path: &Path, reason: &'static str) -> Result<PathBuf, Box<AxumResponse>> {
    path.canonicalize().map_err(|_| run_path_denied(reason))
}

fn run_path_denied(reason: &'static str) -> Box<AxumResponse> {
    Box::new(typed_error(TypedError {
        status: StatusCode::FORBIDDEN,
        code: "workcell_run_path_denied",
        purpose: "run an agent inside a workcell repo slice",
        reason,
        common_fixes: &[
            "claim the workcell with the repo root that contains the program",
            "stage the agent command under the selected repo root before running it",
        ],
        docs_url: "docs/testing.md#workcells",
        repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
        message: reason,
    }))
}

fn driver_error_response(err: DriverError) -> AxumResponse {
    let (status, code, reason) = match err {
        DriverError::Workspace(reason) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "workcell_run_workspace_denied",
            reason,
        ),
        DriverError::Policy(reason) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "workcell_run_policy_denied",
            reason,
        ),
        DriverError::SandboxUnavailable(reason) => (
            StatusCode::FAILED_DEPENDENCY,
            "workcell_run_sandbox_unavailable",
            reason,
        ),
        DriverError::Supervision(reason) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "workcell_run_supervision_failed",
            reason,
        ),
    };
    typed_error(TypedError {
        status,
        code,
        purpose: "run an agent inside a workcell repo slice",
        reason: &reason,
        common_fixes: &[
            "inspect host sandbox capability evidence",
            "rerun the focused workcell_run_agent proof lane",
        ],
        docs_url: "docs/testing.md#workcells",
        repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
        message: &reason,
    })
}

fn is_false(value: &bool) -> bool {
    !*value
}
