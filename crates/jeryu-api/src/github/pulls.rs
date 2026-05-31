//! Pull request routes (`/repos/{owner}/{repo}/pulls...`) and their
//! GitHub-shaped renderers.

use jeryu_core::{
    CreatePullRequestRequest, ForgeError, MergePullRequestRequest, PullRequest, PullRequestState,
};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{
    actor, docs_url, error_response, json_response, owner_json, parse_body, parse_number,
};

impl GithubRouter {
    pub(super) fn list_pulls(&self, owner: &str, repo: &str) -> Response {
        match self.core.list_pull_requests(owner, repo, None) {
            Ok(pulls) => {
                let body: Vec<Value> = pulls.iter().map(pull_request_json).collect();
                json_response(200, &Value::Array(body))
            }
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_pull(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreatePullRequestRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let author = actor(body);
        match self.core.create_pull_request(owner, repo, &author, req) {
            Ok(pr) => json_response(201, &pull_request_json(&pr)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn get_pull(&self, owner: &str, repo: &str, number: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.get_pull_request(owner, repo, number) {
            Ok(pr) => json_response(200, &pull_request_json(&pr)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn merge_pull(&self, owner: &str, repo: &str, number: &str, body: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let req: MergePullRequestRequest = if body.trim().is_empty() {
            MergePullRequestRequest::default()
        } else {
            match parse_body(body) {
                Ok(value) => value,
                Err(response) => return response,
            }
        };
        match self.core.merge_pull_request(owner, repo, number, req) {
            Ok(result) => json_response(
                200,
                &json!({
                    "sha": result.sha,
                    "merged": result.merged,
                    "message": result.message,
                }),
            ),
            // GitHub returns 405 "Method Not Allowed" when a PR is not
            // mergeable (failing checks / protection), distinct from a 404.
            Err(ForgeError::BranchProtection(reason)) => json_response(
                405,
                &json!({ "message": reason, "documentation_url": docs_url() }),
            ),
            Err(err) => error_response(err),
        }
    }
}

pub(super) fn pull_request_json(pr: &PullRequest) -> Value {
    json!({
        "id": pr.id,
        // GitHub-compatible: per-repo `number`, never an internal/global id.
        "number": pr.number,
        "state": pr_open_or_closed(&pr.state),
        "draft": pr.draft,
        "title": pr.title,
        "body": pr.body,
        "user": owner_json(&pr.author),
        "head": git_ref_json(pr, &pr.head),
        "base": git_ref_json(pr, &pr.base),
        "mergeable": pr.mergeable,
        "mergeable_state": pr.mergeable_state,
        "merged": pr.merged,
        "merged_at": pr.merged_at,
        "merge_commit_sha": pr.merge_commit_sha,
        "html_url": format!("/{}/{}/pull/{}", pr.owner, pr.repo, pr.number),
        "url": format!("/repos/{}/{}/pulls/{}", pr.owner, pr.repo, pr.number),
        "created_at": pr.created_at,
        "updated_at": pr.updated_at,
    })
}

fn git_ref_json(pr: &PullRequest, git_ref: &jeryu_core::GitBranchRef) -> Value {
    json!({
        "label": git_ref.label,
        "ref": git_ref.ref_name,
        "sha": git_ref.sha,
        "repo": { "full_name": format!("{}/{}", pr.owner, pr.repo) },
    })
}

/// GitHub PRs only ever report `open`, `closed`, or merged-as-closed at the
/// `state` field. Jeryu's richer lifecycle (Mergeable, BlockedByChecks, ...)
/// is surfaced through `mergeable`/`mergeable_state`; `state` is normalized.
fn pr_open_or_closed(state: &PullRequestState) -> &'static str {
    match state {
        PullRequestState::Merged | PullRequestState::Closed => "closed",
        _ => "open",
    }
}
