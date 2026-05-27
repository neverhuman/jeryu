//! REST handlers for `/api/v1/repos*` (W-B-06 + W-B-07 surface).
//!
//! Exposes:
//!   - `GET    /api/v1/repos`                            list
//!   - `POST   /api/v1/repos/preview`                    preview create
//!   - `POST   /api/v1/repos`                            create
//!   - `GET    /api/v1/repos/{repo_id}`                  detail
//!   - `PATCH  /api/v1/repos/{repo_id}`                  update (subset)
//!
//! Every mutating handler routes through the permissions middleware
//! (`code.write`, `repo.create`, `settings.write` as appropriate). The
//! viewer is extracted from the request extensions (auth layer).

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use jeryu::api::repository::{
    CreateRepositoryPreview, CreateRepositoryRequest, RepositoryListResponse, RepositorySummary,
};

use crate::repos::service::RepoListQuery;
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub struct ListReposParams {
    pub search: Option<String>,
    pub host: Option<String>,
    pub owner: Option<String>,
    pub family: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/repos",
    params(ListReposParams),
    responses(
        (status = 200, description = "Repository list", body = RepositoryListResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "repos",
    security(("session" = [])),
)]
pub async fn list_repos(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Query(params): Query<ListReposParams>,
) -> Result<Json<RepositoryListResponse>, ApiError> {
    require(&viewer, perms::REPO_READ)?;
    let query = RepoListQuery {
        host: params.host,
        owner: params.owner,
        family: params.family,
        search: params.search,
        include_archived: params.include_archived,
        limit: params.limit.unwrap_or(50),
        cursor: params.cursor,
    };
    let resp = state.repo_service.list(query).await?;
    Ok(Json(resp))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RepoDetailResponse {
    #[serde(flatten)]
    pub summary: RepositorySummary,
}

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}",
    params(("repo_id" = String, Path, description = "Stable opaque repo ID")),
    responses(
        (status = 200, description = "Repository detail", body = RepoDetailResponse),
        (status = 404, description = "Repository not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "repos",
    security(("session" = [])),
)]
pub async fn get_repo(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepoDetailResponse>, ApiError> {
    require(&viewer, perms::REPO_READ)?;
    let summary = state.repo_service.get(&repo_id).await?;
    Ok(Json(RepoDetailResponse { summary }))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/preview",
    request_body = CreateRepositoryRequest,
    responses(
        (status = 200, description = "Create preview", body = CreateRepositoryPreview),
        (status = 400, description = "Validation failed"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "repos",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn create_repo_preview(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Json(req): Json<CreateRepositoryRequest>,
) -> Result<Json<CreateRepositoryPreview>, ApiError> {
    require(&viewer, perms::REPO_CREATE)?;
    let preview = state.repo_service.preview_create(&req).await?;
    Ok(Json(preview))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos",
    request_body = CreateRepositoryRequest,
    responses(
        (status = 200, description = "Created repository", body = RepositorySummary),
        (status = 400, description = "Validation failed"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "repos",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn create_repo(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Json(req): Json<CreateRepositoryRequest>,
) -> Result<Json<RepositorySummary>, ApiError> {
    require(&viewer, perms::REPO_CREATE)?;
    let summary = state.repo_service.create(req).await?;
    Ok(Json(summary))
}

#[utoipa::path(
    patch,
    path = "/api/v1/repos/{repo_id}",
    params(("repo_id" = String, Path, description = "Stable opaque repo ID")),
    request_body = crate::repos::settings::SettingsPatch,
    responses(
        (status = 200, description = "Patched repository settings", body = jeryu::api::settings::RepositorySettings),
        (status = 400, description = "Validation failed"),
        (status = 404, description = "Repository not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "repos",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn patch_repo(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Json(patch): Json<crate::repos::settings::SettingsPatch>,
) -> Result<Json<jeryu::api::settings::RepositorySettings>, ApiError> {
    require(&viewer, perms::SETTINGS_WRITE)?;
    // PATCH /api/v1/repos/{repo_id} delegates to SettingsService::apply_patch
    // with an empty base_settings_hash (the user opt-in optimistic-concurrency
    // path lives on /settings; this endpoint is convenience-only).
    let updated = state
        .settings_service
        .apply_patch(&repo_id, "", patch, None)
        .await?;
    Ok(Json(updated))
}
