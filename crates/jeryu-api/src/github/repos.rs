//! Repository routes (`/repos`, `/repos/{owner}/{repo}`) and their
//! GitHub-shaped repository renderer.

use jeryu_core::{CreateRepositoryRequest, Repository};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{
    Pagination, error_response, json_response, owner_for_create, owner_json, paginate, parse_body,
};

impl GithubRouter {
    pub(super) fn list_repos(&self, path: &str, page: Pagination) -> Response {
        let repos = self.core.list_repositories(None);
        let body: Vec<Value> = repos.iter().map(repository_json).collect();
        paginate(path, page, &body, |slice, _total| {
            Value::Array(slice.to_vec())
        })
    }

    pub(super) fn create_repo(&self, body: &str) -> Response {
        let req: CreateRepositoryRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        // GitHub authenticated-user repo creation; the in-memory edge uses the
        // request login when present, defaulting to the canonical owner.
        let owner = owner_for_create(body).unwrap_or_else(|| "jeryu".to_owned());
        match self.core.create_repository(&owner, req) {
            Ok(repo) => json_response(201, &repository_json(&repo)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn get_repo(&self, owner: &str, repo: &str) -> Response {
        match self.core.get_repository(owner, repo) {
            Ok(repo) => json_response(200, &repository_json(&repo)),
            Err(err) => error_response(err),
        }
    }
}

pub(super) fn repository_json(repo: &Repository) -> Value {
    json!({
        "id": repo.id,
        "name": repo.name,
        "full_name": repo.full_name,
        "private": repo.private,
        "owner": owner_json(&repo.owner),
        "description": repo.description,
        "default_branch": repo.default_branch,
        "archived": repo.archived,
        "disabled": repo.disabled,
        "html_url": format!("/{}", repo.full_name),
        "url": format!("/repos/{}", repo.full_name),
        "created_at": repo.created_at,
        "updated_at": repo.updated_at,
    })
}
