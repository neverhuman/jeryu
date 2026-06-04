//! Codegraph oracle REST facade.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_codegraph::{CodegraphQuery, query_store};
use serde_json::json;

use super::WebState;
use super::repositories::find_repo;

pub(super) async fn query(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    if find_repo(&state, &id).is_none() {
        return codegraph_error(
            StatusCode::NOT_FOUND,
            "not_found",
            "query repository codegraph",
            "repository not found for codegraph query",
            &[
                "verify the repository id or owner/name pair",
                "refresh the local forge import before retrying",
            ],
            "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph",
        );
    }
    let request: CodegraphQuery = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            return codegraph_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "codegraph_invalid_request",
                "query repository codegraph",
                &err.to_string(),
                &[
                    "send changed_paths as an array of repo-relative strings",
                    "send symbol and crate_name as strings when filtering the oracle pack",
                ],
                "fix the request body, then rerun the codegraph API proof lane",
            );
        }
    };
    match query_store(&state.codegraph_store, &request) {
        Ok(pack) => Json(pack).into_response(),
        Err(err) => codegraph_error(
            StatusCode::FAILED_DEPENDENCY,
            "codegraph_query_failed",
            "query repository codegraph",
            &err.to_string(),
            &[
                "rerun jeryu-codegraph index before querying",
                "inspect the auxiliary codegraph SQLite store",
            ],
            "rerun cargo test -p jeryu-codegraph -p jeryu-mcp --jobs 40 code",
        ),
    }
}

fn codegraph_error(
    status: StatusCode,
    code: &'static str,
    purpose: &'static str,
    reason: &str,
    common_fixes: &'static [&'static str],
    repair_hint: &'static str,
) -> AxumResponse {
    (
        status,
        Json(json!({
            "code": code,
            "message": reason,
            "purpose": purpose,
            "reason": reason,
            "common_fixes": common_fixes,
            "docs_url": "docs/errors.md#not-found",
            "repair_hint": repair_hint,
        })),
    )
        .into_response()
}
