use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::Json;
use jeryu_core::{CreatePullRequestRequest, ForgeError};
use jeryu_readmodel::dashboards::workcells::{
    WorkcellItem, WorkcellState as ReadWorkcellState, WorkcellsDashboard, WorkcellsSummary,
};
use jeryu_readmodel::TuiReadModel;
use jeryu_runnerd::{FrozenCiSnapshot, StartupSync, WorkcellClaimRequest, WorkcellLease};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{WebState, api_error, server_time};

#[derive(Debug, Deserialize)]
pub(super) struct WorkcellHeartbeatRequest {
    runner_epoch: u64,
    heartbeat_healthy: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct WorkcellReleaseRequest {
    runner_epoch: u64,
}

#[derive(Debug, Deserialize)]
pub(super) struct RepairLiveRequest {
    claim: WorkcellClaimRequest,
    failed_run_id: String,
    failed_receipt_id: String,
    failure_log_digest: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ExportRepairPrRequest {
    workcell_id: String,
    runner_epoch: u64,
    branch_suffix: String,
    owner: String,
    repo: String,
    author: String,
    #[serde(default)]
    target_branch: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RepairLiveResponse {
    held: WorkcellLease,
    repairing: WorkcellLease,
}

#[derive(Debug, Serialize)]
pub(super) struct ExportRepairPrResponse {
    branch: String,
    pull_request_number: u64,
    pull_request_url: String,
    workcell: WorkcellLease,
}

pub(super) async fn list(State(state): State<std::sync::Arc<WebState>>) -> Json<Vec<WorkcellLease>> {
    Json(state.workcells.lock().expect("workcell state").workcells())
}

pub(super) async fn status(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    match state
        .workcells
        .lock()
        .expect("workcell state")
        .workcell(&id)
        .cloned()
    {
        Some(lease) => Json(lease).into_response(),
        None => not_found(&id),
    }
}

pub(super) async fn claim(
    State(state): State<std::sync::Arc<WebState>>,
    body: Bytes,
) -> AxumResponse {
    let request: WorkcellClaimRequest = match parse_json_body(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    match state.workcells.lock().expect("workcell state").claim(request) {
        Ok(lease) => Json(lease).into_response(),
        Err(err) => workcell_error(
            axum::http::StatusCode::CONFLICT,
            &err.reason,
            &err.to_string(),
            err.purpose,
            err.reason,
            err.common_fixes,
            err.docs_url,
            err.repair_hint,
        ),
    }
}

pub(super) async fn heartbeat(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: WorkcellHeartbeatRequest = match parse_json_body(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    match state
        .workcells
        .lock()
        .expect("workcell state")
        .heartbeat(&id, request.runner_epoch, request.heartbeat_healthy)
    {
        Ok(()) => status(State(state), AxumPath(id)).await,
        Err(err) => workcell_error(
            axum::http::StatusCode::CONFLICT,
            &err.reason,
            &err.to_string(),
            err.purpose,
            err.reason,
            err.common_fixes,
            err.docs_url,
            err.repair_hint,
        ),
    }
}

pub(super) async fn release(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: WorkcellReleaseRequest = match parse_json_body(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    match state
        .workcells
        .lock()
        .expect("workcell state")
        .release(&id, request.runner_epoch)
    {
        Ok(()) => status(State(state), AxumPath(id)).await,
        Err(err) => workcell_error(
            axum::http::StatusCode::CONFLICT,
            &err.reason,
            &err.to_string(),
            err.purpose,
            err.reason,
            err.common_fixes,
            err.docs_url,
            err.repair_hint,
        ),
    }
}

pub(super) async fn repair_live(
    State(state): State<std::sync::Arc<WebState>>,
    body: Bytes,
) -> AxumResponse {
    let request: RepairLiveRequest = match parse_json_body(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let mut manager = state.workcells.lock().expect("workcell state");
    match manager.hold_failed_tree(
        request.claim,
        request.failed_run_id,
        request.failed_receipt_id,
        request.failure_log_digest,
    ) {
        Ok(held) => match manager.begin_live_repair(&held.workcell_id, held.runner_epoch) {
            Ok(repairing) => Json(RepairLiveResponse { held, repairing }).into_response(),
            Err(err) => workcell_error(
                axum::http::StatusCode::CONFLICT,
                &err.reason,
                &err.to_string(),
                err.purpose,
                err.reason,
                err.common_fixes,
                err.docs_url,
                err.repair_hint,
            ),
        },
        Err(err) => workcell_error(
            axum::http::StatusCode::CONFLICT,
            &err.reason,
            &err.to_string(),
            err.purpose,
            err.reason,
            err.common_fixes,
            err.docs_url,
            err.repair_hint,
        ),
    }
}

pub(super) async fn export_pr(
    State(state): State<std::sync::Arc<WebState>>,
    body: Bytes,
) -> AxumResponse {
    let request: ExportRepairPrRequest = match parse_json_body(&body) {
        Ok(request) => request,
        Err(response) => return response,
    };

    let mut manager = state.workcells.lock().expect("workcell state");
    let Some(lease) = manager.workcell(&request.workcell_id).cloned() else {
        return not_found(&request.workcell_id);
    };
    if lease.runner_epoch != request.runner_epoch {
        return workcell_error(
            axum::http::StatusCode::CONFLICT,
            "workcell_epoch_fenced",
            &format!(
                "workcell {} fenced: epoch {} != active {}",
                lease.workcell_id, request.runner_epoch, lease.runner_epoch
            ),
            "fence stale workcell epochs",
            "workcell_epoch_fenced",
            &[
                "refresh the workcell lease before retrying the mutation",
                "discard stale heartbeats or releases that carry an old epoch",
            ],
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
        );
    }

    let branch = match manager.export_repair_branch(
        &request.workcell_id,
        request.runner_epoch,
        request.branch_suffix.clone(),
    ) {
        Ok(branch) => branch,
        Err(err) => {
            return workcell_error(
                axum::http::StatusCode::CONFLICT,
                &err.reason,
                &err.to_string(),
                err.purpose,
                err.reason,
                err.common_fixes,
                err.docs_url,
                err.repair_hint,
            );
        }
    };
    drop(manager);

    let (head_sha, base_sha, changed_files) = lease
        .frozen_snapshot
        .as_ref()
        .map(|snapshot| {
            (
                snapshot.head_sha.clone(),
                snapshot.base_sha.clone(),
                snapshot
                    .allowed_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| {
            (
                lease.startup_head_sha.clone().unwrap_or_default(),
                lease.startup_base_sha.clone().unwrap_or_default(),
                lease.allowed_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
            )
        });

    let target_branch = request
        .target_branch
        .clone()
        .or_else(|| lease.startup_main_ref.clone())
        .unwrap_or_else(|| "main".to_string());

    let pr = match state.github.core().create_pull_request(
        &request.owner,
        &request.repo,
        &request.author,
        CreatePullRequestRequest {
            title: request
                .title
                .clone()
                .unwrap_or_else(|| format!("repair {}", lease.workcell_id)),
            body: request.body.clone(),
            head: branch.clone(),
            base: target_branch,
            head_sha: Some(head_sha),
            base_sha: Some(base_sha),
            changed_files,
            ..CreatePullRequestRequest::default()
        },
    ) {
        Ok(pr) => pr,
        Err(ForgeError::NotFound(_)) => {
            return not_found(&format!("{}/{}", request.owner, request.repo))
        }
        Err(err) => {
            return workcell_error(
                axum::http::StatusCode::CONFLICT,
                "workcell_export_failed",
                &err.to_string(),
                "export the repair branch as a pull request",
                "workcell_export_failed",
                &[
                    "verify the repository exists and the repair branch was opened successfully",
                    "refresh the workcell before retrying the export",
                ],
                "docs/boundaries.md#workcells",
                "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            )
        }
    };

    Json(ExportRepairPrResponse {
        branch,
        pull_request_number: pr.number,
        pull_request_url: format!("/repos/{}/{}/pulls/{}", request.owner, request.repo, pr.number),
        workcell: lease,
    })
    .into_response()
}

pub(super) fn live_tui(state: &WebState) -> TuiReadModel {
    let mut model = state.tui.clone();
    model.workcells = dashboard_from_manager(&state.workcells.lock().expect("workcell state"));
    model
}

pub(super) fn dashboard_from_manager(manager: &jeryu_runnerd::WorkcellManager) -> WorkcellsDashboard {
    let items = manager
        .workcells()
        .into_iter()
        .map(|lease| lease_to_item(&lease))
        .collect::<Vec<_>>();
    let summary = Some(WorkcellsSummary {
        total_workcells: items.len() as u32,
        warming_workcells: items
            .iter()
            .filter(|item| item.claim_state == ReadWorkcellState::Warming)
            .count() as u32,
        ready_workcells: items
            .iter()
            .filter(|item| item.claim_state == ReadWorkcellState::Ready)
            .count() as u32,
        claimed_workcells: items
            .iter()
            .filter(|item| item.claim_state == ReadWorkcellState::Claimed)
            .count() as u32,
        held_workcells: items
            .iter()
            .filter(|item| item.claim_state == ReadWorkcellState::Held)
            .count() as u32,
        repairing_workcells: items
            .iter()
            .filter(|item| item.claim_state == ReadWorkcellState::Repairing)
            .count() as u32,
        blocked_workcells: items
            .iter()
            .filter(|item| item.claim_state == ReadWorkcellState::Blocked)
            .count() as u32,
        heartbeat_healthy: items
            .iter()
            .filter(|item| item.heartbeat_healthy)
            .count() as u32,
    });
    WorkcellsDashboard {
        items,
        freshness: None,
        summary,
    }
}

pub(super) fn snapshot_event(
    state: &WebState,
    workcell_id: &str,
) -> Option<jeryu_readmodel::contracts::WebEvent> {
    let lease = state
        .workcells
        .lock()
        .expect("workcell state")
        .workcell(workcell_id)?
        .clone();
    let item = lease_to_item(&lease);
    let seq = state.ws.next_seq();
    Some(jeryu_readmodel::contracts::WebEvent {
        seq,
        timestamp: server_time(),
        scope: format!("workcell.{workcell_id}"),
        kind: "workcell.snapshot".to_string(),
        entity: workcell_id.to_string(),
        summary: format!(
            "workcell {} is {} for {}",
            workcell_id,
            item.claim_state.label(),
            item.label
        ),
        payload: serde_json::to_value(item).ok()?,
    })
}

fn lease_to_item(lease: &WorkcellLease) -> WorkcellItem {
    WorkcellItem {
        cell_id: lease.workcell_id.clone(),
        label: if lease.agent_id.is_empty() {
            lease.workcell_id.clone()
        } else {
            format!("{} / {}", lease.agent_id, lease.workspace_root.display())
        },
        claim_state: map_state(lease.state),
        agent_id: lease.agent_id.clone(),
        workspace_root: lease.workspace_root.display().to_string(),
        repo_roots: lease
            .repo_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        branch_budget: lease.branch_policy.max_branches,
        branches_open: lease.branch_policy.open_branches.len() as u32,
        git_status_summary: lease.git_status_summary.clone(),
        ci_snapshot_age_ms: lease.ci_snapshot_age_ms,
        runner_id: lease.runner_id.clone(),
        runner_epoch: lease.runner_epoch,
        heartbeat_healthy: lease.heartbeat_healthy,
        startup_rebased: lease.startup_rebased,
        startup_main_ref: lease.startup_main_ref.clone(),
        startup_base_sha: lease.startup_base_sha.clone(),
        startup_head_sha: lease.startup_head_sha.clone(),
        failed_run_id: lease.failed_run_id.clone(),
        failed_receipt_id: lease.failed_receipt_id.clone(),
        allowed_paths: lease
            .allowed_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        failure_log_digest: lease.failure_log_digest.clone(),
    }
}

fn map_state(state: jeryu_runnerd::WorkcellState) -> ReadWorkcellState {
    match state {
        jeryu_runnerd::WorkcellState::Warming => ReadWorkcellState::Warming,
        jeryu_runnerd::WorkcellState::Ready => ReadWorkcellState::Ready,
        jeryu_runnerd::WorkcellState::Claimed => ReadWorkcellState::Claimed,
        jeryu_runnerd::WorkcellState::Held => ReadWorkcellState::Held,
        jeryu_runnerd::WorkcellState::Repairing => ReadWorkcellState::Repairing,
        jeryu_runnerd::WorkcellState::Blocked => ReadWorkcellState::Blocked,
        jeryu_runnerd::WorkcellState::Released => ReadWorkcellState::Released,
    }
}

fn not_found(id: &str) -> AxumResponse {
    workcell_error(
        axum::http::StatusCode::NOT_FOUND,
        "not_found",
        &format!("workcell {id} not found"),
        "load workcell state",
        "not_found",
        &[
            "verify the workcell id",
            "claim or repair the workcell before querying it",
        ],
        "docs/errors.md#not-found",
        "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
    )
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<T, AxumResponse> {
    serde_json::from_slice(body).map_err(|err| {
        workcell_error(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            &format!("workcell request body failed validation: {err}"),
            "validate a workcell request body",
            "invalid_input",
            &[
                "send JSON with the required workcell fields",
                "reuse the typed claim or repair request shape from the API",
            ],
            "docs/errors.md#invalid-input",
            "rerun cargo test -p jeryu-api --features web --jobs 40",
        )
    })
}

fn workcell_error(
    status: axum::http::StatusCode,
    code: &str,
    message: &str,
    purpose: &str,
    reason: &str,
    common_fixes: &[&str],
    docs_url: &str,
    repair_hint: &str,
) -> AxumResponse {
    (
        status,
        Json(json!({
            "code": code,
            "message": message,
            "jeryu_repair_hint": {
                "purpose": purpose,
                "reason": reason,
                "common_fixes": common_fixes,
                "docs_url": docs_url,
                "repair_hint": repair_hint,
            }
        })),
    )
        .into_response()
}
