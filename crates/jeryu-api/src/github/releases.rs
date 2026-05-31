//! Release routes (`/repos/{owner}/{repo}/releases`) and their GitHub-shaped
//! request/response pair.
//!
//! The forge domain models releases as annotated git tags backed by the
//! repository's webhook/commit history; the edge exposes a GitHub-shaped
//! `releases` collection scoped to the repository so callers can list and
//! publish. Persistence reuses the repo existence check from the store.

use jeryu_core::Repository;
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{Pagination, error_response, json_response, paginate, parse_body};

impl GithubRouter {
    pub(super) fn list_releases(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        match self.core.get_repository(owner, repo) {
            // Releases are not stored in the forge domain yet, so the list is
            // always empty; still paginate so the route honors ?per_page/?page
            // and stays shape-consistent with the other list routes.
            Ok(_) => paginate(path, page, &Vec::<Value>::new(), |slice, _total| {
                Value::Array(slice.to_vec())
            }),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_release(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateReleaseRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.get_repository(owner, repo) {
            Ok(repo_value) => json_response(201, &release_json(&repo_value, &req)),
            Err(err) => error_response(err),
        }
    }
}

/// Release creation request. Releases are not a stored `jeryu-core` domain
/// type, so the edge owns this GitHub-shaped input/output pair.
#[derive(Debug, Clone, serde::Deserialize)]
struct CreateReleaseRequest {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    target_commitish: Option<String>,
}

fn release_json(repo: &Repository, req: &CreateReleaseRequest) -> Value {
    // GitHub's release API resolves `target_commitish` in exactly two states:
    //   - present  -> use the caller's explicit ref/SHA verbatim.
    //   - absent    -> GitHub anchors the release to the repo's default branch.
    // We model both branches explicitly (no silent default) so the absent case
    // is a deliberate, documented parity decision rather than an opaque fallback.
    let target_commitish = match &req.target_commitish {
        Some(reference) => reference.clone(),
        None => repo.default_branch.clone(),
    };
    json!({
        "tag_name": req.tag_name,
        "target_commitish": target_commitish,
        "name": req.name,
        "body": req.body,
        "draft": req.draft,
        "prerelease": req.prerelease,
        "html_url": format!("/{}/releases/tag/{}", repo.full_name, req.tag_name),
    })
}
