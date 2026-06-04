use std::sync::Arc;
use std::time::Duration;

mod errors;
mod paths;
mod sink;
mod types;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_agentbridge::driver::{AgentDriver, CommandSpec};
use jeryu_runnerd::WorkcellState;

use crate::web::WebState;
use crate::web::workcells_support::{
    TypedError, manager, parse_json_body, typed_error, workcell_not_found,
};
use errors::driver_error_response;
use paths::{resolve_program_in_run_root, selected_run_root};
use sink::SerializingAgentSink;
use types::{AgentWorkcellRunOutcome, AgentWorkcellRunRequest, AgentWorkcellRunResponse};

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
                "release the old workcell before starting a new run",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent",
            message: "the workcell run request used a stale runner epoch",
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
