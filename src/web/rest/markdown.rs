//! `POST /api/v1/markdown/render` — standalone markdown preview (§35.1.8).
//!
//! Used by the SPA for inline previews (MR comments, settings notes, etc.).
//! Reuses the same renderer + sanitizer pipeline as the blob/readme paths
//! so the sanitizer policy is identical everywhere.

use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

use jeryu::api::repo_browser::RenderedMarkdown;
use jeryu::repo_browser::markdown::{MarkdownContext, MarkdownError, render_markdown};

use crate::web::auth::Viewer;
use crate::web::error::ApiError;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MarkdownRenderRequest {
    pub markdown: String,
    #[serde(default)]
    pub context: Option<MarkdownRenderContext>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MarkdownRenderContext {
    #[serde(default, alias = "repoId")]
    pub repo_id: Option<String>,
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    #[serde(default)]
    pub current_path: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MarkdownRenderResponse {
    #[serde(flatten)]
    pub rendered: RenderedMarkdown,
}

#[utoipa::path(
    post,
    path = "/api/v1/markdown/render",
    request_body = MarkdownRenderRequest,
    responses(
        (status = 200, description = "Rendered markdown", body = MarkdownRenderResponse),
        (status = 400, description = "Validation failed"),
        (status = 401, description = "Unauthenticated"),
    ),
    tag = "markdown",
    security(("session" = []), ("csrf" = [])),
)]
pub async fn render_markdown_handler(
    Extension(_viewer): Extension<Viewer>,
    Json(req): Json<MarkdownRenderRequest>,
) -> Result<Json<MarkdownRenderResponse>, ApiError> {
    let ctx_holder = req.context.unwrap_or(MarkdownRenderContext {
        repo_id: None,
        ref_name: None,
        current_path: None,
    });
    let repo_id = ctx_holder.repo_id.as_deref().unwrap_or("");
    let ref_name = ctx_holder.ref_name.as_deref().unwrap_or("main");
    let current_path = ctx_holder.current_path.as_deref().unwrap_or("");
    let ctx = MarkdownContext {
        repo_id,
        ref_name,
        current_path,
    };
    let rendered = render_markdown(&req.markdown, &ctx).map_err(|e| match e {
        MarkdownError::TooLarge(_) => ApiError::ValidationFailed(e.to_string()),
        MarkdownError::RenderFailed(_) => ApiError::BadRequest(e.to_string()),
    })?;
    Ok(Json(MarkdownRenderResponse { rendered }))
}
