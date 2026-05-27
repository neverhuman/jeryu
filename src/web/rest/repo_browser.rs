//! REST handlers for `/api/v1/repos/{repo_id}/refs|tree|blob|raw|readme|compare|history|blame`
//! (W-B-09 + W-B-10 surface).

use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};

use jeryu::api::repo_browser::{BlobResponse, RefSelectorItem, TreeEntry};
use jeryu::git_host::{HostBlame, HostCompare, Page};

use crate::repos::models::RepoId;
use crate::web::auth::Viewer;
use crate::web::error::ApiError;
use crate::web::permissions::{perms, require};
use crate::web::state::WebState;

#[derive(Debug, Deserialize)]
pub struct RefQuery {
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TreeQuery {
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlobQuery {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
    pub render: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RawQuery {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct CompareQuery {
    pub base: String,
    pub head: String,
}

#[derive(Debug, Deserialize)]
pub struct CommitsQuery {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: Option<String>,
    pub per_page: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlameQuery {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct RefsResponse {
    pub refs: Vec<RefSelectorItem>,
}

#[derive(Debug, Serialize)]
pub struct TreeResponse {
    pub entries: Vec<TreeEntry>,
}

#[derive(Debug, Serialize)]
pub struct CompareResponse {
    pub base_sha: String,
    pub head_sha: String,
    pub ahead_by: u32,
    pub behind_by: u32,
    pub files: Vec<CompareFile>,
}

#[derive(Debug, Serialize)]
pub struct CompareFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CommitsResponse {
    pub commits: Vec<CommitItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CommitItem {
    pub sha: String,
    pub author: String,
    pub message: String,
    pub parent_shas: Vec<String>,
    pub committed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct BlameResponse {
    pub lines: Vec<BlameLine>,
}

#[derive(Debug, Serialize)]
pub struct BlameLine {
    pub sha: String,
    pub author: String,
    pub line: u32,
    pub original_line: u32,
    pub content: String,
}

// ── Handlers ──────────────────────────────────────────────────────────

pub async fn list_refs(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
) -> Result<Json<RefsResponse>, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let refs = state
        .browser_service
        .list_refs(&repo)
        .await
        .map_err(map_browser_error)?;
    Ok(Json(RefsResponse { refs }))
}

pub async fn get_tree(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<TreeQuery>,
) -> Result<Json<TreeResponse>, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let path = q.path.as_deref().unwrap_or("");
    let entries = state
        .browser_service
        .tree(&repo, &q.ref_name, path)
        .await
        .map_err(map_browser_error)?;
    Ok(Json(TreeResponse { entries }))
}

pub async fn get_blob(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<BlobQuery>,
) -> Result<Json<BlobResponse>, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let repo_id_dto = jeryu::api::repository::RepositoryId::from(&parsed);
    let response = state
        .browser_service
        .blob(&repo_id_dto, &repo, &q.ref_name, &q.path, q.render.as_deref())
        .await
        .map_err(map_browser_error)?;
    Ok(Json(response))
}

pub async fn get_raw(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<RawQuery>,
) -> Result<Response, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let (bytes, mime, is_binary) = state
        .browser_service
        .raw(&repo, &q.ref_name, &q.path)
        .await
        .map_err(map_browser_error)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        mime.parse().unwrap_or_else(|_| {
            "application/octet-stream"
                .parse()
                .expect("ASCII content-type")
        }),
    );
    if is_binary {
        let filename = q
            .path
            .rsplit('/')
            .next()
            .unwrap_or("download")
            .to_string();
        if let Ok(disp) = format!("attachment; filename=\"{filename}\"").parse() {
            headers.insert(header::CONTENT_DISPOSITION, disp);
        }
    }
    let mut resp = Response::new(Body::from(bytes));
    *resp.status_mut() = StatusCode::OK;
    *resp.headers_mut() = headers;
    Ok(resp)
}

pub async fn get_readme(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<RefQuery>,
) -> Result<Json<BlobResponse>, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let repo_id_dto = jeryu::api::repository::RepositoryId::from(&parsed);
    let response = state
        .browser_service
        .readme(&repo_id_dto, &repo, q.ref_name.as_deref())
        .await
        .map_err(map_browser_error)?;
    Ok(Json(response))
}

pub async fn compare_refs(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<CompareResponse>, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let cmp: HostCompare = state
        .browser_service
        .compare(&repo, &q.base, &q.head)
        .await
        .map_err(map_browser_error)?;
    Ok(Json(CompareResponse {
        base_sha: cmp.base_sha,
        head_sha: cmp.head_sha,
        ahead_by: cmp.ahead_by,
        behind_by: cmp.behind_by,
        files: cmp
            .files
            .into_iter()
            .map(|f| CompareFile {
                path: f.path,
                status: f.status,
                additions: f.additions,
                deletions: f.deletions,
                patch: f.patch,
            })
            .collect(),
    }))
}

pub async fn list_commits(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<CommitsQuery>,
) -> Result<Json<CommitsResponse>, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let page = Page {
        per_page: q.per_page.unwrap_or(50),
        cursor: q.cursor,
    };
    let result = state
        .browser_service
        .commits(&repo, &q.ref_name, q.path.as_deref(), page)
        .await
        .map_err(map_browser_error)?;
    Ok(Json(CommitsResponse {
        commits: result
            .items
            .into_iter()
            .map(|c| CommitItem {
                sha: c.sha,
                author: c.author,
                message: c.message,
                parent_shas: c.parent_shas,
                committed_at: c.committed_at,
            })
            .collect(),
        next_cursor: result.next_cursor,
    }))
}

pub async fn get_blame(
    State(state): State<WebState>,
    Extension(viewer): Extension<Viewer>,
    Path(repo_id): Path<String>,
    Query(q): Query<BlameQuery>,
) -> Result<Json<BlameResponse>, ApiError> {
    require(&viewer, perms::CODE_READ)?;
    let parsed = RepoId::parse(&repo_id)
        .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
    let repo = parsed.repo_ref();
    let blame: HostBlame = state
        .browser_service
        .blame(&repo, &q.ref_name, &q.path)
        .await
        .map_err(map_browser_error)?;
    Ok(Json(BlameResponse {
        lines: blame
            .lines
            .into_iter()
            .map(|l| BlameLine {
                sha: l.sha,
                author: l.author,
                line: l.line,
                original_line: l.original_line,
                content: l.content,
            })
            .collect(),
    }))
}

fn map_browser_error(err: jeryu::repo_browser::BrowserError) -> ApiError {
    use jeryu::repo_browser::BrowserError;
    match err {
        BrowserError::Invalid(msg) => ApiError::ValidationFailed(msg),
        BrowserError::NotFound(msg) => ApiError::NotFound(msg),
        BrowserError::UpstreamForbidden(msg) => ApiError::UpstreamForbidden(msg),
        BrowserError::RateLimited => ApiError::RateLimited,
        BrowserError::Upstream(msg) => ApiError::Upstream(msg),
    }
}
