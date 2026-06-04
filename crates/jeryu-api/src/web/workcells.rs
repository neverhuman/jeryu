use std::path::PathBuf;
use std::sync::Arc;

mod export_pr;
mod run_agent;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_readmodel::contracts::WebEvent;
use jeryu_readmodel::{TuiReadModel, WorkcellsDashboard, WorkcellsSummary};
use jeryu_runnerd::{HoldFailedTreeRequest, StartupSync, WorkcellClaimRequest, WorkcellLease};
use serde::{Deserialize, Serialize};

use super::WebState;
use super::surface::serialize_payload;
use super::workcells_support::{
    default_true, lease_to_item, manager, parse_json_body, workcell_error, workcell_not_found,
};

pub(super) use export_pr::export_pr;
pub(super) use run_agent::run_agent;

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
    #[serde(default)]
    pub ci_run_id: Option<String>,
    pub failed_run_id: String,
    pub failed_receipt_id: String,
    pub failure_log_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RepairLiveResponse {
    pub held: WorkcellLease,
    pub repairing: WorkcellLease,
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
    let failed_run_id = request.failed_run_id;
    let ci_run_id = request.ci_run_id.unwrap_or_else(|| failed_run_id.clone());
    let mut manager = manager(&state);
    let held = match manager.hold_failed_tree(HoldFailedTreeRequest {
        claim,
        ci_run_id,
        failed_run_id,
        failed_receipt_id: request.failed_receipt_id,
        failure_log_digest: request.failure_log_digest,
    }) {
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
