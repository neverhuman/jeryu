//! REST handler for global search (W-B-16).
//!
//! `GET /api/v1/search?q=&kinds=repo,file,commit,mr,issue,user&limit=`
//!
//! Backed by `crate::repos::search::SearchService`. Phase 4 implements
//! repo search live via `RepoService::list`; other kinds return empty
//! vectors with a stable shape so the SPA can render the section list
//! today and pick up cache hits once W-F-05+ populates them.

use axum::{
    Extension, Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::repos::search::{SearchKind, SearchResults, SearchService};
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: Option<String>,
    pub kinds: Option<String>,
    pub limit: Option<u32>,
}

pub async fn search(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchResults>, ApiError> {
    require(&viewer, perms::REPO_READ)?;
    let kinds = SearchKind::parse_list(q.kinds.as_deref())?;
    let limit = q.limit.unwrap_or(20).min(100);
    let needle = q.q.unwrap_or_default();
    let svc = SearchService::new(state.repo_service.clone());
    let results = svc.search(&needle, &kinds, limit).await?;
    Ok(Json(results))
}
