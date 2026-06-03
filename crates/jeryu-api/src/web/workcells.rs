use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::{CreatePullRequestRequest, ForgeError};
use jeryu_readmodel::contracts::WebEvent;
use jeryu_readmodel::{TuiReadModel, WorkcellsDashboard, WorkcellsSummary};
use jeryu_runnerd::{
    StartupSync, WorkcellClaimRequest, WorkcellError, WorkcellLease, WorkcellManager,
    WorkcellState as RunnerWorkcellState,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::WebState;
use super::surface::serialize_payload;

type WorkcellParseResult<T> = Result<T, Box<AxumResponse>>;

struct TypedError<'a> {
    status: StatusCode,
    code: &'a str,
    purpose: &'a str,
    reason: &'a str,
    common_fixes: &'a [&'a str],
    docs_url: &'a str,
    repair_hint: &'a str,
    message: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WorkcellHeartbeatRequest {
    pub runner_epoch: u64,
    #[serde(default = "default_true")]
    pub heartbeat_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct WorkcellReleaseRequest {
    pub runner_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RepairLiveRequest {
    pub agent_id: String,
    pub workspace_root: PathBuf,
    pub repo_roots: Vec<PathBuf>,
    pub branch_budget: u32,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub git_status_summary: String,
    #[serde(default)]
    pub ci_snapshot_age_ms: Option<u64>,
    pub startup: StartupSync,
    pub failed_run_id: String,
    pub failed_receipt_id: String,
    pub failure_log_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ExportRepairPrRequest {
    pub workcell_id: String,
    pub runner_epoch: u64,
    pub branch_suffix: String,
    pub owner: String,
    pub repo: String,
    pub author: String,
    #[serde(default)]
    pub target_branch: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RepairLiveResponse {
    pub held: WorkcellLease,
    pub repairing: WorkcellLease,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ExportRepairPrResponse {
    pub workcell_id: String,
    pub branch: String,
    pub target_branch: String,
    pub pull_request_number: u64,
}

pub(super) async fn list(State(state): State<Arc<WebState>>) -> Json<Vec<WorkcellLease>> {
    Json(manager(&state).workcells())
}

pub(super) async fn status(
    State(state): State<Arc<WebState>>,
    AxumPath(workcell_id): AxumPath<String>,
) -> AxumResponse {
    match manager(&state).workcell(&workcell_id).cloned() {
        Some(lease) => Json(lease).into_response(),
        None => workcell_not_found(&workcell_id),
    }
}

pub(super) async fn claim(State(state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    let request: WorkcellClaimRequest = match parse_json_body(
        &body,
        "claim a ready workcell",
        "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    let outcome = manager(&state).claim(request);
    match outcome {
        Ok(lease) => Json(lease).into_response(),
        Err(err) => workcell_error(err),
    }
}

pub(super) async fn repair_live(State(state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    let request: RepairLiveRequest = match parse_json_body(
        &body,
        "hold a failed workcell and start live repair",
        "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    let claim = WorkcellClaimRequest {
        agent_id: request.agent_id,
        workspace_root: request.workspace_root,
        repo_roots: request.repo_roots,
        branch_budget: request.branch_budget,
        runner_id: request.runner_id,
        runner_epoch: request.runner_epoch,
        git_status_summary: request.git_status_summary,
        ci_snapshot_age_ms: request.ci_snapshot_age_ms,
        startup: request.startup,
    };
    let mut manager = manager(&state);
    let held = match manager.hold_failed_tree(
        claim,
        request.failed_run_id,
        request.failed_receipt_id,
        request.failure_log_digest,
    ) {
        Ok(lease) => lease,
        Err(err) => return workcell_error(err),
    };
    let repairing = match manager.begin_live_repair(&held.workcell_id, held.runner_epoch) {
        Ok(lease) => lease,
        Err(err) => return workcell_error(err),
    };
    Json(RepairLiveResponse { held, repairing }).into_response()
}

pub(super) async fn heartbeat(
    State(state): State<Arc<WebState>>,
    AxumPath(workcell_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: WorkcellHeartbeatRequest = match parse_json_body(
        &body,
        "refresh a workcell heartbeat",
        "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    let mut manager = manager(&state);
    match manager.heartbeat(
        &workcell_id,
        request.runner_epoch,
        request.heartbeat_healthy,
    ) {
        Ok(()) => match manager.workcell(&workcell_id).cloned() {
            Some(lease) => Json(lease).into_response(),
            None => workcell_not_found(&workcell_id),
        },
        Err(err) => workcell_error(err),
    }
}

pub(super) async fn release(
    State(state): State<Arc<WebState>>,
    AxumPath(workcell_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: WorkcellReleaseRequest = match parse_json_body(
        &body,
        "release a workcell lease",
        "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    let mut manager = manager(&state);
    match manager.release(&workcell_id, request.runner_epoch) {
        Ok(()) => match manager.workcell(&workcell_id).cloned() {
            Some(lease) => Json(lease).into_response(),
            None => workcell_not_found(&workcell_id),
        },
        Err(err) => workcell_error(err),
    }
}

pub(super) async fn export_pr(
    State(state): State<Arc<WebState>>,
    AxumPath(workcell_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: ExportRepairPrRequest = match parse_json_body(
        &body,
        "export a repair branch into a pull request",
        "rerun cargo test -p jeryu-api --features web --jobs 40",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    if request.workcell_id != workcell_id {
        return typed_error(TypedError {
            status: StatusCode::BAD_REQUEST,
            code: "workcell_id_mismatch",
            purpose: "export a repair branch into a pull request",
            reason: "request path and body disagreed on the workcell id",
            common_fixes: &[
                "send the same workcell id in the path and request body",
                "reload the workcell status before retrying the export",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
            message: "the request body did not match the selected workcell",
        });
    }

    let mut manager = manager(&state);
    let branch = match manager.export_repair_branch(
        &workcell_id,
        request.runner_epoch,
        request.branch_suffix,
    ) {
        Ok(branch) => branch,
        Err(err) => return workcell_error(err),
    };

    let lease = match manager.workcell(&workcell_id).cloned() {
        Some(lease) => lease,
        None => return workcell_not_found(&workcell_id),
    };
    let target_branch = request
        .target_branch
        .or_else(|| lease.startup_main_ref.clone())
        .unwrap_or_else(|| "main".to_string());
    let title = request
        .title
        .unwrap_or_else(|| format!("Repair {}", lease.workcell_id));
    let body = request.body.or_else(|| {
        Some(format!(
            "Workcell: {}\nFailure log: {}\n",
            lease.workcell_id,
            lease
                .failure_log_digest
                .clone()
                .or_else(|| lease
                    .frozen_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.failure_log_digest.clone()))
                .unwrap_or_else(|| "unknown".to_string())
        ))
    });
    let snapshot = lease.frozen_snapshot.as_ref();
    let head_sha = snapshot
        .map(|snapshot| snapshot.head_sha.clone())
        .or_else(|| lease.startup_head_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let base_sha = snapshot
        .map(|snapshot| snapshot.base_sha.clone())
        .or_else(|| lease.startup_base_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let changed_files = Vec::new();
    let pr = match state.github.core().create_pull_request(
        &request.owner,
        &request.repo,
        &request.author,
        CreatePullRequestRequest {
            title,
            body,
            head: branch.clone(),
            base: target_branch.clone(),
            head_sha: Some(head_sha),
            base_sha: Some(base_sha),
            draft: false,
            commits: Vec::new(),
            changed_files,
        },
    ) {
        Ok(pr) => pr,
        Err(err) => return forge_error(err),
    };

    (
        StatusCode::CREATED,
        Json(ExportRepairPrResponse {
            workcell_id,
            branch,
            target_branch,
            pull_request_number: pr.number,
        }),
    )
        .into_response()
}

pub(super) fn live_tui(state: &WebState) -> TuiReadModel {
    let mut tui = state.tui.clone();
    tui.workcells = dashboard_from_manager(state);
    tui
}

pub(super) fn dashboard_from_manager(state: &WebState) -> WorkcellsDashboard {
    let manager = manager(state);
    let items: Vec<_> = manager.workcells().into_iter().map(lease_to_item).collect();
    let summary = Some(WorkcellsSummary {
        total_workcells: items.len() as u32,
        warming_workcells: items
            .iter()
            .filter(|item| item.claim_state == jeryu_readmodel::WorkcellState::Warming)
            .count() as u32,
        ready_workcells: items
            .iter()
            .filter(|item| item.claim_state == jeryu_readmodel::WorkcellState::Ready)
            .count() as u32,
        claimed_workcells: items
            .iter()
            .filter(|item| item.claim_state == jeryu_readmodel::WorkcellState::Claimed)
            .count() as u32,
        held_workcells: items
            .iter()
            .filter(|item| item.claim_state == jeryu_readmodel::WorkcellState::Held)
            .count() as u32,
        repairing_workcells: items
            .iter()
            .filter(|item| item.claim_state == jeryu_readmodel::WorkcellState::Repairing)
            .count() as u32,
        blocked_workcells: items
            .iter()
            .filter(|item| item.claim_state == jeryu_readmodel::WorkcellState::Blocked)
            .count() as u32,
        heartbeat_healthy: items.iter().filter(|item| item.heartbeat_healthy).count() as u32,
    });
    WorkcellsDashboard {
        items,
        freshness: None,
        summary,
    }
}

pub(super) fn snapshot_event(state: &WebState, workcell_id: &str) -> Option<WebEvent> {
    let lease = manager(state).workcell(workcell_id)?.clone();
    let item = lease_to_item(lease);
    let seq = state.ws.next_seq();
    Some(WebEvent {
        seq,
        timestamp: super::server_time(),
        scope: format!("workcell.{workcell_id}"),
        kind: "workcell.snapshot".to_string(),
        entity: workcell_id.to_string(),
        summary: format!("workcell '{}' is {}", workcell_id, item.claim_state.label()),
        payload: serialize_payload(&item).ok()?,
    })
}

fn lease_to_item(lease: WorkcellLease) -> jeryu_readmodel::WorkcellItem {
    let mut item = jeryu_readmodel::WorkcellItem::new(
        lease.workcell_id.clone(),
        if lease.agent_id.is_empty() {
            lease.workcell_id.clone()
        } else {
            format!("{} / {}", lease.agent_id, lease.workspace_root.display())
        },
    );
    item.claim_state = map_state(lease.state);
    item.agent_id = lease.agent_id;
    item.repo_roots = lease
        .repo_roots
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    item.workspace_root = lease.workspace_root.to_string_lossy().to_string();
    item.branch_budget = lease.branch_policy.max_branches;
    item.branches_open = lease.branch_policy.open_branches.len() as u32;
    item.git_status_summary = lease.git_status_summary;
    item.ci_snapshot_age_ms = lease.ci_snapshot_age_ms;
    item.runner_id = lease.runner_id;
    item.runner_epoch = lease.runner_epoch;
    item.heartbeat_healthy = lease.heartbeat_healthy;
    item.startup_rebased = lease.startup_rebased;
    item.startup_main_ref = lease.startup_main_ref;
    item.startup_base_sha = lease.startup_base_sha;
    item.startup_head_sha = lease.startup_head_sha;
    item.failed_run_id = lease.failed_run_id;
    item.failed_receipt_id = lease.failed_receipt_id;
    item.allowed_paths = lease
        .allowed_paths
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    item.failure_log_digest = lease.failure_log_digest;
    item
}

fn map_state(state: RunnerWorkcellState) -> jeryu_readmodel::WorkcellState {
    match state {
        RunnerWorkcellState::Warming => jeryu_readmodel::WorkcellState::Warming,
        RunnerWorkcellState::Ready => jeryu_readmodel::WorkcellState::Ready,
        RunnerWorkcellState::Claimed => jeryu_readmodel::WorkcellState::Claimed,
        RunnerWorkcellState::Held => jeryu_readmodel::WorkcellState::Held,
        RunnerWorkcellState::Repairing => jeryu_readmodel::WorkcellState::Repairing,
        RunnerWorkcellState::Blocked => jeryu_readmodel::WorkcellState::Blocked,
        RunnerWorkcellState::Released => jeryu_readmodel::WorkcellState::Released,
    }
}

fn workcell_error(err: WorkcellError) -> AxumResponse {
    let status = match err.reason {
        "workcell_tar_path_denied" => StatusCode::UNPROCESSABLE_ENTITY,
        "workcell_epoch_fenced" => StatusCode::CONFLICT,
        "workcell_repair_state_denied" => StatusCode::CONFLICT,
        "workcell_startup_rebase_failed" => StatusCode::CONFLICT,
        "workcell_branch_budget_denied" => StatusCode::CONFLICT,
        "workcell_claim_denied" => StatusCode::CONFLICT,
        "workcell_merge_denied" => StatusCode::FORBIDDEN,
        "workcell_delete_denied" => StatusCode::FORBIDDEN,
        _ => StatusCode::BAD_REQUEST,
    };
    typed_error(TypedError {
        status,
        code: err.reason,
        purpose: err.purpose,
        reason: err.message(),
        common_fixes: err.common_fixes,
        docs_url: err.docs_url,
        repair_hint: err.repair_hint,
        message: err.message(),
    })
}

fn forge_error(err: ForgeError) -> AxumResponse {
    let (status, reason, common_fixes, repair_hint) = match &err {
        ForgeError::NotFound(_) => (
            StatusCode::NOT_FOUND,
            "forge_not_found",
            &[
                "confirm the owner and repository exist",
                "use the local API or bootstrap snapshot to find the live repo name",
            ][..],
            "retry with a live owner/repo pair, then rerun cargo test -p jeryu-api --features web",
        ),
        ForgeError::Conflict(_) => (
            StatusCode::CONFLICT,
            "forge_conflict",
            &[
                "refresh the workcell before retrying",
                "resolve the current branch state and retry the export",
            ][..],
            "refresh the workcell state, then rerun cargo test -p jeryu-api --features web",
        ),
        ForgeError::Validation(_) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "forge_validation",
            &[
                "fix the request fields",
                "rebuild the request through the typed API or MCP surface",
            ][..],
            "rerun the request after the body is corrected",
        ),
        ForgeError::BranchProtection(_) => (
            StatusCode::METHOD_NOT_ALLOWED,
            "forge_branch_protection",
            &[
                "use the review queue for merge operations",
                "keep merge control out of the workcell export path",
            ][..],
            "route merges through the review flow, then retry the export if needed",
        ),
        ForgeError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "forge_storage",
            &[
                "check the local forge data directory",
                "retry after the storage issue is resolved",
            ][..],
            "rerun after the forge storage backend is healthy",
        ),
    };
    let message = err.to_string();
    typed_error(TypedError {
        status,
        code: reason,
        purpose: "export a repair pull request",
        reason: &message,
        common_fixes,
        docs_url: "docs/testing.md#workcells",
        repair_hint,
        message: &message,
    })
}

fn parse_json_body<T: DeserializeOwned>(
    body: &Bytes,
    purpose: &'static str,
    repair_hint: &'static str,
) -> WorkcellParseResult<T> {
    serde_json::from_slice(body).map_err(|err| {
        let message = err.to_string();
        Box::new(typed_error(TypedError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "workcell_invalid_request",
            purpose,
            reason: &message,
            common_fixes: &[
                "send a JSON body that matches the route schema",
                "use the typed MCP/API surface to build the request",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint,
            message: &message,
        }))
    })
}

fn typed_error(error: TypedError<'_>) -> AxumResponse {
    (
        error.status,
        Json(json!({
            "code": error.code,
            "message": error.message,
            "purpose": error.purpose,
            "reason": error.reason,
            "common_fixes": error.common_fixes,
            "docs_url": error.docs_url,
            "repair_hint": error.repair_hint,
        })),
    )
        .into_response()
}

fn workcell_not_found(workcell_id: &str) -> AxumResponse {
    let message = format!("workcell {workcell_id} was not found");
    typed_error(TypedError {
        status: StatusCode::NOT_FOUND,
        code: "not_found",
        purpose: "inspect a workcell",
        reason: &message,
        common_fixes: &[
            "claim a workcell before asking for its status",
            "reload the workcells list and retry with a live id",
        ],
        docs_url: "docs/testing.md#workcells",
        repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
        message: &message,
    })
}

fn manager(state: &WebState) -> std::sync::MutexGuard<'_, WorkcellManager> {
    state.workcells.lock().expect("workcell manager lock")
}

fn default_true() -> bool {
    true
}
