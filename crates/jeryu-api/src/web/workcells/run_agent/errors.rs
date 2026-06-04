use axum::http::StatusCode;
use axum::response::Response as AxumResponse;
use jeryu_agentbridge::driver::DriverError;

use crate::web::workcells_support::{TypedError, typed_error};

pub(super) fn driver_error_response(err: DriverError) -> AxumResponse {
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
