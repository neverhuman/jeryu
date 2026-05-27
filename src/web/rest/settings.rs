//! REST handlers for `/api/v1/repos/{repo_id}/settings*` (W-B-07).
//!
//! Routes:
//!   - `GET   /api/v1/repos/{repo_id}/settings`
//!   - `POST  /api/v1/repos/{repo_id}/settings/preview`
//!   - `PATCH /api/v1/repos/{repo_id}/settings`     (If-Match required)

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::HeaderMap,
};

use jeryu::api::settings::RepositorySettings;

use crate::repos::settings::{SettingsDiffPreview, SettingsPatch};
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

#[utoipa::path(
    get,
    path = "/api/v1/repos/{repo_id}/settings",
    params(("repo_id" = String, Path, description = "Stable opaque repo ID")),
    responses(
        (status = 200, description = "Repository settings", body = RepositorySettings),
        (status = 404, description = "Repository not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "settings",
    security(("session" = [])),
)]
pub async fn get_settings(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
) -> Result<Json<RepositorySettings>, ApiError> {
    require(&viewer, perms::SETTINGS_READ)?;
    let settings = state.settings_service.read(&repo_id).await?;
    Ok(Json(settings))
}

#[utoipa::path(
    post,
    path = "/api/v1/repos/{repo_id}/settings/preview",
    params(("repo_id" = String, Path, description = "Stable opaque repo ID")),
    request_body = SettingsPatch,
    responses(
        (status = 200, description = "Settings patch diff preview", body = SettingsDiffPreview),
        (status = 400, description = "Validation failed"),
        (status = 404, description = "Repository not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "settings",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn preview_settings_patch(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<SettingsDiffPreview>, ApiError> {
    require(&viewer, perms::SETTINGS_WRITE)?;
    let preview = state
        .settings_service
        .preview_patch(&repo_id, &patch)
        .await?;
    Ok(Json(preview))
}

#[utoipa::path(
    patch,
    path = "/api/v1/repos/{repo_id}/settings",
    params(("repo_id" = String, Path, description = "Stable opaque repo ID")),
    request_body = SettingsPatch,
    responses(
        (status = 200, description = "Updated settings", body = RepositorySettings),
        (status = 400, description = "Validation failed"),
        (status = 409, description = "Concurrent update (If-Match hash mismatch)"),
        (status = 404, description = "Repository not found"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden"),
    ),
    tag = "settings",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn patch_settings(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    headers: HeaderMap,
    Json(patch): Json<SettingsPatch>,
) -> Result<Json<RepositorySettings>, ApiError> {
    require(&viewer, perms::SETTINGS_WRITE)?;
    // If-Match header carries the base settings hash. Quoted-string per
    // RFC 9110; the BFF unwraps a single optional `"…"` pair.
    let base_hash = headers
        .get(axum::http::header::IF_MATCH)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_default();
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let updated = state
        .settings_service
        .apply_patch(&repo_id, &base_hash, patch, idempotency_key.as_deref())
        .await?;
    Ok(Json(updated))
}
