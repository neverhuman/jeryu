//! Fast tool-building insight routes backed by the codegraph SQLite store.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use serde::Deserialize;
use serde_json::json;

use super::WebState;
use super::workcells_support::{TypedError, typed_error};

const TOOL_BUILD_DOCS: &str = "docs/codegraph-tool-build.md";
const TOOL_BUILD_RERUN: &str = "rerun cargo test -p jeryu-api --features web --jobs 40 tool_build";

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ToolBuildQuery {
    pub(super) repo: Option<String>,
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) include_ignored: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ToolBuildFeedbackRequest {
    reason: String,
    #[serde(default)]
    ignored_by: Option<String>,
}

pub(super) async fn status(
    State(state): State<Arc<WebState>>,
    Query(query): Query<ToolBuildQuery>,
) -> AxumResponse {
    match state
        .codegraph_store
        .tool_build_cluster_counts(query.repo.as_deref())
    {
        Ok((cluster_count, ignored_count)) => Json(json!({
            "schema_version": "codegraph.tool_build/v1",
            "repo": query.repo,
            "ready": true,
            "cluster_count": cluster_count,
            "ignored_count": ignored_count,
        }))
        .into_response(),
        Err(error) => tool_build_storage_error(&error.to_string()),
    }
}

pub(super) async fn clusters(
    State(state): State<Arc<WebState>>,
    Query(query): Query<ToolBuildQuery>,
) -> AxumResponse {
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    match state.codegraph_store.tool_build_clusters(
        query.repo.as_deref(),
        limit,
        query.include_ignored,
    ) {
        Ok(clusters) => Json(json!({
            "schema_version": "codegraph.tool_build/v1",
            "repo": query.repo,
            "include_ignored": query.include_ignored,
            "clusters": clusters,
        }))
        .into_response(),
        Err(error) => tool_build_storage_error(&error.to_string()),
    }
}

pub(super) async fn feedback(
    State(state): State<Arc<WebState>>,
    AxumPath(cluster_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: ToolBuildFeedbackRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return tool_build_typed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "tool_build_invalid_request",
                "record tool-building cluster feedback",
                &error.to_string(),
                &[
                    "send JSON with a non-empty reason field",
                    "use jeryu.codegraph.tool_build.feedback from MCP",
                ],
                "fix the feedback body, then rerun the tool_build proof lane",
            );
        }
    };
    if request.reason.trim().is_empty() {
        return tool_build_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "tool_build_feedback_reason_required",
            "record tool-building cluster feedback",
            "ignore feedback requires a non-empty reason",
            &[
                "explain why this cluster should not produce a Jankurai tool-building task",
                "use include_ignored=true when auditing suppressed clusters",
            ],
            "add a reason, then rerun the tool_build proof lane",
        );
    }
    let ignored_by = request.ignored_by.as_deref().unwrap_or("api");
    match state.codegraph_store.ignore_tool_build_cluster(
        &cluster_id,
        request.reason.trim(),
        ignored_by,
    ) {
        Ok(ignored) => Json(ignored).into_response(),
        Err(error) => tool_build_storage_error(&error.to_string()),
    }
}

fn tool_build_storage_error(reason: &str) -> AxumResponse {
    tool_build_typed_error(
        StatusCode::FAILED_DEPENDENCY,
        "tool_build_store_unavailable",
        "query tool-building codegraph insights",
        reason,
        &[
            "run jeryu-codegraph tool-build scan before querying clusters",
            "verify the codegraph SQLite store path is writable",
        ],
        TOOL_BUILD_RERUN,
    )
}

fn tool_build_typed_error(
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
        docs_url: TOOL_BUILD_DOCS,
        repair_hint,
        message: reason,
    })
}
