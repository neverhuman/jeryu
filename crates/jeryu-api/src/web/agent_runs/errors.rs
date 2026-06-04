use axum::http::StatusCode;
use axum::response::Response as AxumResponse;
use jeryu_agent_auth::AgentAuthError;

use crate::web::workcells_support::{TypedError, typed_error};

pub(super) fn stream_error(err: jeryu_agent_stream::AgentStreamError) -> AxumResponse {
    typed_error(TypedError {
        status: StatusCode::FAILED_DEPENDENCY,
        code: &err.code,
        purpose: &err.repair.purpose,
        reason: &err.repair.reason,
        common_fixes: refs(&err.repair.common_fixes).as_slice(),
        docs_url: &err.repair.docs_url,
        repair_hint: &err.repair.repair_hint,
        message: &err.repair.reason,
    })
}

pub(super) fn auth_error(err: AgentAuthError) -> AxumResponse {
    typed_error(TypedError {
        status: StatusCode::FAILED_DEPENDENCY,
        code: &err.code,
        purpose: &err.repair.purpose,
        reason: &err.repair.reason,
        common_fixes: refs(&err.repair.common_fixes).as_slice(),
        docs_url: &err.repair.docs_url,
        repair_hint: &err.repair.repair_hint,
        message: &err.repair.reason,
    })
}

pub(super) fn agent_run_not_found(agent_run_id: &str) -> AxumResponse {
    agent_typed_error(
        StatusCode::NOT_FOUND,
        "not_found",
        "inspect an agent-edit run",
        format!("agent run {agent_run_id} was not found"),
        &[
            "start an agent run before checking status or control",
            "reload the run id from the agent-runs response",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    )
}

pub(super) fn agent_typed_error(
    status: StatusCode,
    code: &'static str,
    purpose: &'static str,
    reason: impl Into<String>,
    common_fixes: &'static [&'static str],
    docs_url: &'static str,
    repair_hint: &'static str,
) -> AxumResponse {
    let reason = reason.into();
    typed_error(TypedError {
        status,
        code,
        purpose,
        reason: &reason,
        common_fixes,
        docs_url,
        repair_hint,
        message: &reason,
    })
}

fn refs(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}
