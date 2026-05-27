//! REST handlers for `/api/v1/repos/{repo_id}/merge-requests*` (W-B-11/13).
//!
//! Endpoint map (mirrors §35.7):
//!   - `GET    /api/v1/repos/{id}/merge-requests?state=`         list
//!   - `POST   /api/v1/repos/{id}/merge-requests`                create (501 stub)
//!   - `GET    /api/v1/repos/{id}/merge-requests/{iid}`          detail (with passport)
//!   - `PATCH  /api/v1/repos/{id}/merge-requests/{iid}`          update (501 stub)
//!   - `GET    /api/v1/repos/{id}/merge-requests/{iid}/diff`     diff
//!   - `GET    /api/v1/repos/{id}/merge-requests/{iid}/checks`   CI checks
//!   - `GET    /api/v1/repos/{id}/merge-requests/{iid}/blockers` blockers (= passport)
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/approve`  (Idempotency-Key)
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/request-changes`
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/merge`    (Idempotency-Key)
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/close`
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/reopen`
//!   - `POST   /api/v1/repos/{id}/merge-requests/{iid}/rebase`
//!
//! Every mutating handler:
//!   - extracts the viewer from `Extension<Viewer>`,
//!   - calls `require(&viewer, ...)` with the canonical permission key,
//!   - reads the optional `Idempotency-Key` header for replay,
//!   - threads the actor (`viewer.login`) into the service so the audit row
//!     can identify the principal.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};

use jeryu::api::merge_request::{MergePassport, MergeRequestDetail, MergeRequestSummary};
use jeryu::api::review::SubmitReviewRequest;

use crate::merge::service::MergeListQuery;
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub struct ListMrParams {
    pub state: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListMrResponse {
    pub merge_requests: Vec<MergeRequestSummary>,
}

#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
pub struct CreateMrRequest {
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Default, utoipa::ToSchema)]
pub struct PatchMrRequest {
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ApproveMrRequest {
    pub expected_head_sha: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RequestChangesRequest {
    pub expected_head_sha: String,
    #[serde(default)]
    pub body_markdown: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MergeMrRequest {
    pub expected_head_sha: String,
    /// `"merge"`, `"squash"`, or `"rebase"`. Defaults to `"merge"`.
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub commit_title: Option<String>,
    #[serde(default)]
    pub commit_message: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApproveMrResponse {
    pub mr_iid: String,
    pub head_sha: String,
    pub receipt: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MergeMrResponse {
    pub mr_iid: String,
    pub head_sha: String,
    pub receipt: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EmptyOk {
    pub ok: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DiffResponse {
    pub repo: String,
    pub mr_iid: String,
    pub head_sha: String,
    pub base_sha: String,
    pub files: Vec<DiffFile>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DiffFile {
    pub path: String,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub hunks: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = MrChecksResponse)]
pub struct ChecksResponse {
    pub mr_iid: String,
    pub head_sha: String,
    pub pipelines: Vec<PipelineSummary>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PipelineSummary {
    pub id: String,
    pub status: String,
    pub web_url: Option<String>,
}

// ── Handlers ───────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/merge-requests",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ListMrParams,
    ),
    responses(
        (status = 200, description = "Merge requests list", body = ListMrResponse),
        (status = 404, description = "Repository not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = [])),
)]
pub async fn list_merge_requests(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(params): Query<ListMrParams>,
) -> Result<Json<ListMrResponse>, ApiError> {
    require(&viewer, perms::MR_READ)?;
    let merge_requests = state
        .merge_service
        .list(
            &repo_id,
            MergeListQuery {
                state: params.state,
            },
        )
        .await?;
    Ok(Json(ListMrResponse { merge_requests }))
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    responses(
        (status = 200, description = "Merge request detail", body = MergeRequestDetail),
        (status = 404, description = "MR not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = [])),
)]
pub async fn get_merge_request(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<MergeRequestDetail>, ApiError> {
    require(&viewer, perms::MR_READ)?;
    let detail = state.merge_service.get(&repo_id, &iid).await?;
    Ok(Json(detail))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/merge-requests",
    params(("repo_id" = String, Path, description = "Stable opaque repo ID")),
    request_body = CreateMrRequest,
    responses(
        (status = 200, description = "Stub OK (Phase 3 placeholder)", body = EmptyOk),
        (status = 502, description = "Upstream adapter not implemented"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn create_merge_request(
    State(_state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(_repo_id): Path<String>,
    headers: HeaderMap,
    Json(_req): Json<CreateMrRequest>,
) -> Result<Json<EmptyOk>, ApiError> {
    require(&viewer, perms::MR_WRITE)?;
    let _ = idempotency_key(&headers)
        .ok_or_else(|| ApiError::BadRequest("Idempotency-Key header is required".into()))?;
    // Phase 3 stub: the host adapter doesn't yet expose MR creation; surface
    // as 502 upstream_unavailable so the UI can degrade.
    Err(ApiError::Upstream(
        "mr.create not implemented in Phase 3 (host adapter pending)".into(),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    request_body = PatchMrRequest,
    responses(
        (status = 200, description = "Stub OK (Phase 3 placeholder)", body = EmptyOk),
        (status = 502, description = "Upstream adapter not implemented"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn patch_merge_request(
    State(_state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((_repo_id, _iid)): Path<(String, String)>,
    Json(_req): Json<PatchMrRequest>,
) -> Result<Json<EmptyOk>, ApiError> {
    require(&viewer, perms::MR_WRITE)?;
    Err(ApiError::Upstream(
        "mr.update not implemented in Phase 3 (host adapter pending)".into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/approve",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    request_body = ApproveMrRequest,
    responses(
        (status = 200, description = "Approval receipt", body = ApproveMrResponse),
        (status = 400, description = "Validation failed / SHA drift"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn approve_merge_request(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
    headers: HeaderMap,
    Json(req): Json<ApproveMrRequest>,
) -> Result<Json<ApproveMrResponse>, ApiError> {
    require(&viewer, perms::MR_APPROVE)?;
    let key = idempotency_key(&headers)
        .ok_or_else(|| ApiError::BadRequest("Idempotency-Key header is required".into()))?;
    let result = state
        .merge_service
        .approve_exact_sha(&repo_id, &iid, &req.expected_head_sha, &viewer.login, Some(&key))
        .await?;
    Ok(Json(ApproveMrResponse {
        mr_iid: result.mr_iid,
        head_sha: result.head_sha,
        receipt: result.receipt,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/request-changes",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    request_body = RequestChangesRequest,
    responses(
        (status = 200, description = "Review submitted", body = EmptyOk),
        (status = 400, description = "Validation failed / SHA drift"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn request_changes(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
    Json(req): Json<RequestChangesRequest>,
) -> Result<Json<EmptyOk>, ApiError> {
    require(&viewer, perms::MR_REVIEW)?;
    state
        .review_service
        .submit_review(
            &repo_id,
            &iid,
            SubmitReviewRequest {
                verdict: jeryu::api::review::ReviewVerdict::RequestChanges,
                expected_head_sha: req.expected_head_sha,
                body_markdown: req.body_markdown,
                thread_comments: vec![],
            },
            &viewer.login,
            None,
        )
        .await?;
    Ok(Json(EmptyOk { ok: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/merge",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    request_body = MergeMrRequest,
    responses(
        (status = 200, description = "Merge receipt", body = MergeMrResponse),
        (status = 400, description = "Validation failed / SHA drift"),
        (status = 409, description = "Merge blockers present"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn merge_merge_request(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
    headers: HeaderMap,
    Json(req): Json<MergeMrRequest>,
) -> Result<Json<MergeMrResponse>, ApiError> {
    require(&viewer, perms::MR_MERGE)?;
    let key = idempotency_key(&headers)
        .ok_or_else(|| ApiError::BadRequest("Idempotency-Key header is required".into()))?;
    let method = req.method.as_deref().unwrap_or("merge");
    let result = state
        .merge_service
        .merge_exact_sha(
            &repo_id,
            &iid,
            &req.expected_head_sha,
            method,
            req.commit_title.as_deref(),
            req.commit_message.as_deref(),
            &viewer.login,
            Some(&key),
        )
        .await?;
    Ok(Json(MergeMrResponse {
        mr_iid: result.mr_iid,
        head_sha: result.head_sha,
        receipt: result.receipt,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/close",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    responses(
        (status = 200, description = "Closed", body = EmptyOk),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn close_merge_request(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<EmptyOk>, ApiError> {
    require(&viewer, perms::MR_WRITE)?;
    state.merge_service.close_mr(&repo_id, &iid, &viewer.login).await?;
    Ok(Json(EmptyOk { ok: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/reopen",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    responses(
        (status = 200, description = "Reopened", body = EmptyOk),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn reopen_merge_request(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<EmptyOk>, ApiError> {
    require(&viewer, perms::MR_WRITE)?;
    state.merge_service.reopen_mr(&repo_id, &iid, &viewer.login).await?;
    Ok(Json(EmptyOk { ok: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/rebase",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    responses(
        (status = 200, description = "Rebase queued", body = EmptyOk),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn rebase_merge_request(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<EmptyOk>, ApiError> {
    require(&viewer, perms::MR_WRITE)?;
    state.merge_service.rebase_mr(&repo_id, &iid, &viewer.login).await?;
    Ok(Json(EmptyOk { ok: true }))
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/diff",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    responses(
        (status = 200, description = "MR diff", body = DiffResponse),
        (status = 404, description = "MR not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = [])),
)]
pub async fn get_diff(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<DiffResponse>, ApiError> {
    require(&viewer, perms::MR_READ)?;
    let diff = state.merge_service.diff(&repo_id, &iid).await?;
    Ok(Json(DiffResponse {
        repo: diff.repo,
        mr_iid: diff.mr_iid,
        head_sha: diff.head_sha,
        base_sha: diff.base_sha,
        files: diff
            .changed_files
            .into_iter()
            .map(|f| DiffFile {
                path: f.path,
                lines_added: f.lines_added,
                lines_removed: f.lines_removed,
                hunks: f.hunks,
            })
            .collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/checks",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    responses(
        (status = 200, description = "MR checks summary", body = ChecksResponse),
        (status = 404, description = "MR not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = [])),
)]
pub async fn get_checks(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<ChecksResponse>, ApiError> {
    require(&viewer, perms::MR_READ)?;
    let detail = state.merge_service.get(&repo_id, &iid).await?;
    // Use the existing passport_service to surface the latest pipeline on
    // the head SHA. Phase 3: report the single pipeline matched by SHA;
    // future work paginates over all pipelines.
    let pipes = jeryu::git_host::GitHost::list_pipelines(
        state.gitlab_client.as_ref(),
        &crate::repos::models::RepoId::parse(&repo_id)
            .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?
            .repo_ref(),
        Some(&detail.summary.head_sha),
        jeryu::git_host::Page {
            per_page: 20,
            cursor: None,
        },
    )
    .await
    .map_err(crate::repos::service::host_to_api_error)?;
    Ok(Json(ChecksResponse {
        mr_iid: detail.summary.iid,
        head_sha: detail.summary.head_sha,
        pipelines: pipes
            .items
            .into_iter()
            .map(|p| PipelineSummary {
                id: p.id,
                status: p.status,
                web_url: p.web_url,
            })
            .collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/merge-requests/{iid}/blockers",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("iid" = String, Path, description = "MR internal ID"),
    ),
    responses(
        (status = 200, description = "Merge passport (blockers)", body = MergePassport),
        (status = 404, description = "MR not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "merge_requests",
    security(("session" = [])),
)]
pub async fn get_blockers(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, iid)): Path<(String, String)>,
) -> Result<Json<MergePassport>, ApiError> {
    require(&viewer, perms::MR_READ)?;
    let parsed = crate::repos::models::RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let passport = state.passport_service.compute(&parsed, &iid).await?;
    Ok(Json(passport))
}

fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Idempotency-Key")
        .or_else(|| headers.get("idempotency-key"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn idempotency_key_returns_none_when_absent() {
        let h = HeaderMap::new();
        assert!(idempotency_key(&h).is_none());
    }

    #[test]
    fn idempotency_key_reads_lowercase_or_canonical() {
        let mut h = HeaderMap::new();
        h.insert("idempotency-key", HeaderValue::from_static("abc"));
        assert_eq!(idempotency_key(&h).as_deref(), Some("abc"));
    }
}
