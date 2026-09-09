use std::sync::MutexGuard;

use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::ForgeError;
use jeryu_runnerd::{WorkcellError, WorkcellLease, WorkcellManager};
use serde::de::DeserializeOwned;
use serde_json::json;

use super::WebState;

const RETIRED_JEKKO_ROOT: &str = "/home/ubuntu/jekko";
const ACTIVE_JEKKO_ROOT: &str = "/home/ubuntu/jekko-split/jekko";

pub(super) fn normalize_deprecated_host_path(path: &std::path::Path) -> std::path::PathBuf {
    let retired = std::path::Path::new(RETIRED_JEKKO_ROOT);
    match path.strip_prefix(retired) {
        Ok(rest) if rest.as_os_str().is_empty() => std::path::PathBuf::from(ACTIVE_JEKKO_ROOT),
        Ok(rest) => std::path::Path::new(ACTIVE_JEKKO_ROOT).join(rest),
        Err(_) => path.to_path_buf(),
    }
}

pub(super) struct TypedError<'a> {
    pub status: StatusCode,
    pub code: &'a str,
    pub purpose: &'a str,
    pub reason: &'a str,
    pub common_fixes: &'a [&'a str],
    pub docs_url: &'a str,
    pub repair_hint: &'a str,
    pub message: &'a str,
}

pub(super) fn parse_json_body<T: DeserializeOwned>(
    body: &Bytes,
    purpose: &'static str,
    repair_hint: &'static str,
) -> Result<T, Box<AxumResponse>> {
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

pub(super) fn workcell_error(err: WorkcellError) -> AxumResponse {
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

pub(super) fn forge_error(err: ForgeError) -> AxumResponse {
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

pub(super) fn workcell_not_found(workcell_id: &str) -> AxumResponse {
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

pub(super) fn manager(state: &WebState) -> MutexGuard<'_, WorkcellManager> {
    state.workcells.lock().expect("workcell manager lock")
}

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn typed_error(error: TypedError<'_>) -> AxumResponse {
    (
        error.status,
        axum::Json(json!({
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

pub(super) fn lease_to_item(lease: WorkcellLease) -> jeryu_readmodel::WorkcellItem {
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

pub(super) fn map_state(state: jeryu_runnerd::WorkcellState) -> jeryu_readmodel::WorkcellState {
    match state {
        jeryu_runnerd::WorkcellState::Warming => jeryu_readmodel::WorkcellState::Warming,
        jeryu_runnerd::WorkcellState::Ready => jeryu_readmodel::WorkcellState::Ready,
        jeryu_runnerd::WorkcellState::Claimed => jeryu_readmodel::WorkcellState::Claimed,
        jeryu_runnerd::WorkcellState::Held => jeryu_readmodel::WorkcellState::Held,
        jeryu_runnerd::WorkcellState::Repairing => jeryu_readmodel::WorkcellState::Repairing,
        jeryu_runnerd::WorkcellState::Blocked => jeryu_readmodel::WorkcellState::Blocked,
        jeryu_runnerd::WorkcellState::Released => jeryu_readmodel::WorkcellState::Released,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::normalize_deprecated_host_path;

    #[test]
    fn normalize_deprecated_host_path_moves_retired_jekko_prefix() {
        assert_eq!(
            normalize_deprecated_host_path(Path::new("/home/ubuntu/jekko")),
            Path::new("/home/ubuntu/jekko-split/jekko")
        );
        assert_eq!(
            normalize_deprecated_host_path(Path::new("/home/ubuntu/jekko/jnoccio-fusion")),
            Path::new("/home/ubuntu/jekko-split/jekko/jnoccio-fusion")
        );
        assert_eq!(
            normalize_deprecated_host_path(Path::new("/home/ubuntu/.jekko")),
            Path::new("/home/ubuntu/.jekko")
        );
        assert_eq!(
            normalize_deprecated_host_path(Path::new("/home/ubuntu/jekko-split/jekko")),
            Path::new("/home/ubuntu/jekko-split/jekko")
        );
    }
}
