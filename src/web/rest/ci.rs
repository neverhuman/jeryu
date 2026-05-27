//! REST handlers for CI/pipelines/jobs/checks (W-B-14).
//!
//! Exposes (per §35.7):
//!   - `GET    /api/v1/repos/{repo_id}/pipelines`              list
//!   - `GET    /api/v1/repos/{repo_id}/pipelines/{pipeline_id}` detail
//!   - `GET    /api/v1/repos/{repo_id}/pipelines/{pipeline_id}/jobs` list jobs
//!   - `GET    /api/v1/repos/{repo_id}/jobs/{job_id}/log`      raw log
//!   - `POST   /api/v1/repos/{repo_id}/jobs/{job_id}/retry`    retry (idem.)
//!   - `POST   /api/v1/repos/{repo_id}/jobs/{job_id}/cancel`   cancel (idem.)
//!   - `GET    /api/v1/repos/{repo_id}/checks?sha=`            checks for a SHA
//!
//! Reads require `ci.read`; mutations require `ci.write` + an
//! `Idempotency-Key` header. Mutations write an audit event and publish
//! WS events on the bus per §35.7 / W-B-14 acceptance.

#![allow(clippy::collapsible_if)]

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use jeryu::api::websocket::WebEvent;
use jeryu::git_host::{GitHost, HostJob, HostJobLog, HostPipeline, Page};

use crate::repos::models::RepoId;
use crate::repos::service::host_to_api_error;
use crate::web::audit::{RiskTier, write_audit};
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

// ── Wire DTOs ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListPipelinesQuery {
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PipelineItem {
    pub id: String,
    pub ref_name: String,
    pub sha: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
}

impl From<HostPipeline> for PipelineItem {
    fn from(p: HostPipeline) -> Self {
        Self {
            id: p.id,
            ref_name: p.ref_name,
            sha: p.sha,
            status: p.status,
            created_at: p.created_at,
            updated_at: p.updated_at,
            web_url: p.web_url,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PipelinesResponse {
    pub pipelines: Vec<PipelineItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct JobItem {
    pub id: String,
    pub name: String,
    pub stage: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
}

impl From<HostJob> for JobItem {
    fn from(j: HostJob) -> Self {
        Self {
            id: j.id,
            name: j.name,
            stage: j.stage,
            status: j.status,
            started_at: j.started_at,
            finished_at: j.finished_at,
            duration_secs: j.duration_secs,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct JobsResponse {
    pub jobs: Vec<JobItem>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct JobLogQuery {
    /// Byte offset for incremental fetch; clients pass the previous
    /// response's `next_cursor`. Phase 4: log is fetched in full and the
    /// cursor slices it client-side (best-effort).
    pub cursor: Option<u64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct JobLogResponse {
    /// Log bytes as UTF-8 (lossy if invalid).
    pub bytes: String,
    pub truncated: bool,
    /// Byte offset where the next fetch should resume; absent when the log
    /// is fully drained.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<u64>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ChecksQuery {
    pub sha: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CheckItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChecksResponse {
    pub sha: String,
    pub checks: Vec<CheckItem>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct JobActionReceipt {
    pub job_id: String,
    pub action: String,
    pub accepted: bool,
    pub event_seq: u64,
}

// ── Handlers ──────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/pipelines",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ListPipelinesQuery,
    ),
    responses(
        (status = 200, description = "Pipelines page", body = PipelinesResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "ci",
    security(("session" = [])),
)]
pub async fn list_pipelines(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<ListPipelinesQuery>,
) -> Result<Json<PipelinesResponse>, ApiError> {
    require(&viewer, perms::CI_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let page = Page {
        per_page: q.limit.unwrap_or(50),
        cursor: q.cursor,
    };
    let result = GitHost::list_pipelines(
        state.gitlab_client.as_ref(),
        &repo,
        q.ref_name.as_deref(),
        page,
    )
    .await
    .map_err(host_to_api_error)?;
    let _ = q.status; // server-side filter is host-native; ignored client-side
    let pipelines: Vec<PipelineItem> = result.items.into_iter().map(Into::into).collect();
    Ok(Json(PipelinesResponse {
        pipelines,
        next_cursor: result.next_cursor,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/pipelines/{pipeline_id}",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("pipeline_id" = String, Path, description = "Pipeline ID"),
    ),
    responses(
        (status = 200, description = "Pipeline detail", body = PipelineItem),
        (status = 404, description = "Pipeline not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "ci",
    security(("session" = [])),
)]
pub async fn get_pipeline(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, pipeline_id)): Path<(String, String)>,
) -> Result<Json<PipelineItem>, ApiError> {
    require(&viewer, perms::CI_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    // Phase 4: GitHost has no dedicated `get_pipeline` — list jobs first
    // (cheap proxy) and fall back to listing pipelines and finding the id.
    let pipelines = GitHost::list_pipelines(
        state.gitlab_client.as_ref(),
        &repo,
        None,
        Page {
            per_page: 100,
            cursor: None,
        },
    )
    .await
    .map_err(host_to_api_error)?;
    let item = pipelines
        .items
        .into_iter()
        .find(|p| p.id == pipeline_id)
        .ok_or_else(|| ApiError::NotFound(format!("pipeline {pipeline_id}")))?;
    Ok(Json(item.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/pipelines/{pipeline_id}/jobs",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("pipeline_id" = String, Path, description = "Pipeline ID"),
    ),
    responses(
        (status = 200, description = "Pipeline jobs list", body = JobsResponse),
        (status = 404, description = "Pipeline not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "ci",
    security(("session" = [])),
)]
pub async fn list_pipeline_jobs(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, pipeline_id)): Path<(String, String)>,
) -> Result<Json<JobsResponse>, ApiError> {
    require(&viewer, perms::CI_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let jobs = GitHost::list_jobs(state.gitlab_client.as_ref(), &repo, &pipeline_id)
        .await
        .map_err(host_to_api_error)?;
    Ok(Json(JobsResponse {
        jobs: jobs.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/jobs/{job_id}/log",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("job_id" = String, Path, description = "Job ID"),
        JobLogQuery,
    ),
    responses(
        (status = 200, description = "Job log slice", body = JobLogResponse),
        (status = 404, description = "Job not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "ci",
    security(("session" = [])),
)]
pub async fn get_job_log(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, job_id)): Path<(String, String)>,
    Query(q): Query<JobLogQuery>,
) -> Result<Json<JobLogResponse>, ApiError> {
    require(&viewer, perms::CI_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let HostJobLog { bytes, truncated } =
        GitHost::get_job_log(state.gitlab_client.as_ref(), &repo, &job_id)
            .await
            .map_err(host_to_api_error)?;
    let cursor = q.cursor.unwrap_or(0) as usize;
    let total = bytes.len();
    let slice_start = cursor.min(total);
    let slice = &bytes[slice_start..];
    let text = String::from_utf8_lossy(slice).into_owned();
    let next_cursor = if !truncated && slice_start + slice.len() >= total {
        None
    } else {
        Some((slice_start + slice.len()) as u64)
    };
    Ok(Json(JobLogResponse {
        bytes: text,
        truncated,
        next_cursor,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/jobs/{job_id}/retry",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("job_id" = String, Path, description = "Job ID"),
    ),
    responses(
        (status = 200, description = "Retry accepted", body = JobActionReceipt),
        (status = 400, description = "Missing Idempotency-Key"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "ci",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn retry_job(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, job_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<JobActionReceipt>, ApiError> {
    let idem = require_idempotency_key(&headers)?;
    require(&viewer, perms::CI_WRITE)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    job_action_receipt(&state, &viewer, &parsed, &job_id, "retry", &idem).await
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/jobs/{job_id}/cancel",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ("job_id" = String, Path, description = "Job ID"),
    ),
    responses(
        (status = 200, description = "Cancel accepted", body = JobActionReceipt),
        (status = 400, description = "Missing Idempotency-Key"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "ci",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn cancel_job(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path((repo_id, job_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<JobActionReceipt>, ApiError> {
    let idem = require_idempotency_key(&headers)?;
    require(&viewer, perms::CI_WRITE)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    job_action_receipt(&state, &viewer, &parsed, &job_id, "cancel", &idem).await
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/checks",
    params(
        ("repo_id" = String, Path, description = "Stable opaque repo ID"),
        ChecksQuery,
    ),
    responses(
        (status = 200, description = "Checks at SHA", body = ChecksResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "ci",
    security(("session" = [])),
)]
pub async fn list_checks(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<ChecksQuery>,
) -> Result<Json<ChecksResponse>, ApiError> {
    require(&viewer, perms::CI_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    // Phase 4: derive checks from `list_pipelines` filtered to the SHA.
    let pipelines = GitHost::list_pipelines(
        state.gitlab_client.as_ref(),
        &repo,
        None,
        Page {
            per_page: 100,
            cursor: None,
        },
    )
    .await
    .map_err(host_to_api_error)?;
    let checks: Vec<CheckItem> = pipelines
        .items
        .into_iter()
        .filter(|p| p.sha == q.sha)
        .map(|p| CheckItem {
            id: p.id.clone(),
            name: format!("pipeline:{}", p.ref_name),
            status: p.status,
            sha: p.sha,
            web_url: p.web_url,
        })
        .collect();
    Ok(Json(ChecksResponse { sha: q.sha, checks }))
}

// ── Helpers ──────────────────────────────────────────────────────────

fn require_idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let v = headers
        .get("idempotency-key")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::BadRequest("Idempotency-Key header is required".into()))?;
    Ok(v.to_string())
}

async fn job_action_receipt(
    state: &WebState,
    viewer: &Viewer,
    repo_id: &RepoId,
    job_id: &str,
    action: &'static str,
    idempotency_key: &str,
) -> Result<Json<JobActionReceipt>, ApiError> {
    // Idempotency replay path (process-local cache).
    let cache_key = format!("ci:{}:{}:{}", action, repo_id.encode(), idempotency_key);
    if let Some(stored) = state.idempotency.find(&cache_key) {
        if let Ok(receipt) = serde_json::from_value::<JobActionReceipt>(stored) {
            return Ok(Json(receipt));
        }
    }

    // Phase 4: there is no GitHost mutation for retry/cancel yet — accept
    // the request, emit an audit + event, and persist the receipt so
    // retries replay. The host-side write lands when W-H-06b ships
    // `retry_job` / `cancel_job` on `GitHost`.
    let kind = match action {
        "retry" => "workflow.run.started",
        "cancel" => "check.completed",
        _ => "check.completed",
    };
    let scope = format!("repo.{}", repo_id.encode());
    let payload = serde_json::json!({
        "repo_id": repo_id.encode(),
        "job_id": job_id,
        "action": action,
        "actor": viewer.login,
    });
    let event_seq = state.event_bus.publish(WebEvent {
        seq: 0,
        timestamp: Utc::now(),
        scope: scope.clone(),
        kind: kind.into(),
        entity: format!("job:{}", job_id),
        summary: format!("{action} job {job_id}"),
        payload: payload.clone(),
    });
    if let Err(err) = write_audit(
        &state.db_pool,
        &viewer.login,
        &format!("ci.{action}"),
        &format!("job:{}", job_id),
        RiskTier::Medium,
        payload,
    )
    .await
    {
        tracing::warn!(
            target: "jeryu.web.audit",
            error = %err,
            action,
            job_id,
            "audit event write failed (ci action)"
        );
    }

    let receipt = JobActionReceipt {
        job_id: job_id.to_string(),
        action: action.to_string(),
        accepted: true,
        event_seq,
    };
    state.idempotency.store(
        cache_key,
        serde_json::to_value(&receipt).unwrap_or_default(),
    );
    Ok(Json(receipt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn idempotency_key_required() {
        let headers = HeaderMap::new();
        assert!(matches!(
            require_idempotency_key(&headers),
            Err(ApiError::BadRequest(_))
        ));
    }

    #[test]
    fn idempotency_key_extracted() {
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", HeaderValue::from_static("abc-123"));
        let v = require_idempotency_key(&headers).unwrap();
        assert_eq!(v, "abc-123");
    }

    #[test]
    fn pipeline_item_round_trips_from_host() {
        let now = Utc::now();
        let h = HostPipeline {
            id: "42".into(),
            ref_name: "main".into(),
            sha: "abc".into(),
            status: "success".into(),
            created_at: now,
            updated_at: now,
            web_url: Some("https://gitlab.example/x/y/-/pipelines/42".into()),
        };
        let item: PipelineItem = h.into();
        assert_eq!(item.id, "42");
        assert_eq!(item.ref_name, "main");
        assert_eq!(item.status, "success");
    }

    #[test]
    fn job_item_round_trips_from_host() {
        let h = HostJob {
            id: "j1".into(),
            name: "build".into(),
            stage: "build".into(),
            status: "running".into(),
            started_at: None,
            finished_at: None,
            duration_secs: None,
        };
        let item: JobItem = h.into();
        assert_eq!(item.id, "j1");
        assert!(item.duration_secs.is_none());
    }
}
