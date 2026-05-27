//! REST handlers for review threads + comments (W-B-12 surface).
//!
//! Endpoint map (mirrors §35.7):
//!   - `GET    /api/v1/repos/{id}/merge-requests/{iid}/threads`              list
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/threads`              create
//!   - `PATCH  /api/v1/repos/{id}/merge-requests/{iid}/threads/{thread_id}`  resolve
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/comments`             follow-up
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/reviews`              submit verdict

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use jeryu::api::review::{
    CreateReviewCommentRequest, ReviewComment, ReviewThread, SubmitReviewRequest,
};

use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

#[derive(Debug, Serialize)]
pub struct ListThreadsResponse {
    pub threads: Vec<ReviewThread>,
}

#[derive(Debug, Deserialize)]
pub struct PatchThreadRequest {
    pub resolved: bool,
}

#[derive(Debug, Serialize)]
pub struct SubmitReviewResponse {
    pub review_id: String,
    pub state: String,
    pub head_sha: String,
}

pub async fn list_threads(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<ListThreadsResponse>, ApiError> {
    require(&viewer, perms::MR_READ)?;
    let threads = state.review_service.list_threads(&repo_id, &iid).await?;
    Ok(Json(ListThreadsResponse { threads }))
}

pub async fn create_thread(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
    headers: HeaderMap,
    Json(req): Json<CreateReviewCommentRequest>,
) -> Result<Json<ReviewThread>, ApiError> {
    require(&viewer, perms::MR_COMMENT)?;
    let key = idempotency_key(&headers);
    let thread = state
        .review_service
        .create_thread(&repo_id, &iid, req, &viewer.login, key.as_deref())
        .await?;
    Ok(Json(thread))
}

pub async fn patch_thread(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid, thread_id)): Path<(String, String, String)>,
    Json(req): Json<PatchThreadRequest>,
) -> Result<Json<ReviewThread>, ApiError> {
    require(&viewer, perms::MR_COMMENT)?;
    let thread = state
        .review_service
        .resolve_thread(&repo_id, &iid, &thread_id, req.resolved, &viewer.login)
        .await?;
    Ok(Json(thread))
}

pub async fn create_comment(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
    headers: HeaderMap,
    Json(req): Json<CreateReviewCommentRequest>,
) -> Result<Json<ReviewComment>, ApiError> {
    require(&viewer, perms::MR_COMMENT)?;
    let key = idempotency_key(&headers);
    let comment = state
        .review_service
        .create_comment(&repo_id, &iid, req, &viewer.login, key.as_deref())
        .await?;
    Ok(Json(comment))
}

pub async fn submit_review(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
    headers: HeaderMap,
    Json(req): Json<SubmitReviewRequest>,
) -> Result<Json<SubmitReviewResponse>, ApiError> {
    require(&viewer, perms::MR_REVIEW)?;
    // Approve verdict additionally requires mr.approve permission.
    if matches!(req.verdict, jeryu::api::review::ReviewVerdict::Approve) {
        require(&viewer, perms::MR_APPROVE)?;
    }
    let key = idempotency_key(&headers);
    let result = state
        .review_service
        .submit_review(&repo_id, &iid, req, &viewer.login, key.as_deref())
        .await?;
    Ok(Json(SubmitReviewResponse {
        review_id: result.review_id,
        state: result.state,
        head_sha: result.head_sha,
    }))
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Idempotency-Key")
        .or_else(|| headers.get("idempotency-key"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}
