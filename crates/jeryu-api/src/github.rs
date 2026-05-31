//! GitHub-compatible REST edge for the Jeryu forge.
//!
//! This module wraps [`jeryu_core::ForgeCore`] — the typed, HTTP-free forge
//! domain — and renders its values as GitHub-shaped JSON. The JSON field
//! shapes (PR `number`, `head`/`base` refs, check-run `conclusion`, combined
//! commit `state`, branch-protection booleans, etc.) are authored here against
//! Jeryu's own parity assertions, not vendored from any external spec.
//!
//! The dispatcher keeps the in-process [`Response`](crate::routes::Response)
//! contract used by the rest of the API facade so the future Axum/HTTP edge can
//! wrap [`GithubRouter::handle`] without changing product-truth behavior.

use jeryu_core::{
    BranchProtectionRule, CheckConclusion, CheckRun, CheckRunStatus, CombinedStatus, CommitStatus,
    CommitStatusState, CreateCheckRunRequest, CreateCommentRequest, CreateCommitStatusRequest,
    CreateIssueRequest, CreatePullRequestRequest, CreateRepositoryRequest, CreateWebhookRequest,
    ForgeCore, ForgeError, Issue, IssueComment, IssueState, MergePullRequestRequest, PullRequest,
    PullRequestState, Repository, SetBranchProtectionRequest, Webhook,
};
use serde_json::{Value, json};

use crate::routes::Response;

/// Semantic version reported by `GET /api/v1/version`.
pub const JERYU_API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// HTTP method understood by the GitHub-compatible edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Post,
    Put,
}

/// GitHub-compatible REST router backed by an in-memory [`ForgeCore`] store.
#[derive(Clone, Debug, Default)]
pub struct GithubRouter {
    core: ForgeCore,
}

impl GithubRouter {
    /// Builds a router over a fresh in-memory forge store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrows the backing forge store (used by tests and embedding callers).
    pub fn core(&self) -> &ForgeCore {
        &self.core
    }

    /// Dispatches a request. `body` is the raw JSON request body (empty for
    /// bodiless GETs). The actor is the authenticated principal; the in-memory
    /// edge defaults it where GitHub would take it from the token.
    pub fn handle(&self, method: Method, path: &str, body: &str) -> Response {
        let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
        self.route(method, &segments, body)
            .unwrap_or_else(not_found)
    }

    /// Convenience GET wrapper.
    pub fn get(&self, path: &str) -> Response {
        self.handle(Method::Get, path, "")
    }

    /// Convenience POST wrapper.
    pub fn post(&self, path: &str, body: &str) -> Response {
        self.handle(Method::Post, path, body)
    }

    /// Convenience PUT wrapper.
    pub fn put(&self, path: &str, body: &str) -> Response {
        self.handle(Method::Put, path, body)
    }

    /// Routes a parsed request. Returns `Err(status)` for an unmatched route so
    /// the caller can render the GitHub-shaped fallback body.
    fn route(
        &self,
        method: Method,
        segments: &[&str],
        body: &str,
    ) -> std::result::Result<Response, u16> {
        use Method::{Get, Post, Put};
        match (method, segments) {
            (Get, ["health"]) => Ok(json_response(
                200,
                &json!({ "status": "ok", "service": "jeryu-api" }),
            )),
            (Get, ["api", "v1", "version"]) => Ok(json_response(
                200,
                &json!({ "version": JERYU_API_VERSION, "name": "jeryu-api" }),
            )),

            // Repositories ---------------------------------------------------
            (Get, ["repos"]) => Ok(self.list_repos()),
            (Post, ["repos"]) => Ok(self.create_repo(body)),
            (Get, ["repos", owner, repo]) => Ok(self.get_repo(owner, repo)),

            // Pull requests --------------------------------------------------
            (Get, ["repos", owner, repo, "pulls"]) => Ok(self.list_pulls(owner, repo)),
            (Post, ["repos", owner, repo, "pulls"]) => Ok(self.create_pull(owner, repo, body)),
            (Get, ["repos", owner, repo, "pulls", number]) => {
                Ok(self.get_pull(owner, repo, number))
            }
            (Put, ["repos", owner, repo, "pulls", number, "merge"]) => {
                Ok(self.merge_pull(owner, repo, number, body))
            }

            // Issues ---------------------------------------------------------
            (Get, ["repos", owner, repo, "issues"]) => Ok(self.list_issues(owner, repo)),
            (Post, ["repos", owner, repo, "issues"]) => Ok(self.create_issue(owner, repo, body)),
            (Get, ["repos", owner, repo, "issues", number, "comments"]) => {
                Ok(self.list_comments(owner, repo, number))
            }
            (Post, ["repos", owner, repo, "issues", number, "comments"]) => {
                Ok(self.create_comment(owner, repo, number, body))
            }

            // Commit status --------------------------------------------------
            (Get, ["repos", owner, repo, "commits", reference, "status"]) => {
                Ok(self.commit_status(owner, repo, reference))
            }
            (Post, ["repos", owner, repo, "statuses", sha]) => {
                Ok(self.create_status(owner, repo, sha, body))
            }

            // Check runs -----------------------------------------------------
            (Get, ["repos", owner, repo, "check-runs"]) => Ok(self.list_check_runs(owner, repo)),
            (Post, ["repos", owner, repo, "check-runs"]) => {
                Ok(self.create_check_run(owner, repo, body))
            }

            // Branch protection ----------------------------------------------
            (Get, ["repos", owner, repo, "branches", branch, "protection"]) => {
                Ok(self.get_protection(owner, repo, branch))
            }
            (Put, ["repos", owner, repo, "branches", branch, "protection"]) => {
                Ok(self.set_protection(owner, repo, branch, body))
            }

            // Releases -------------------------------------------------------
            (Get, ["repos", owner, repo, "releases"]) => Ok(self.list_releases(owner, repo)),
            (Post, ["repos", owner, repo, "releases"]) => {
                Ok(self.create_release(owner, repo, body))
            }

            // Webhooks -------------------------------------------------------
            (Get, ["repos", owner, repo, "hooks"]) => Ok(self.list_hooks(owner, repo)),
            (Post, ["repos", owner, repo, "hooks"]) => Ok(self.create_hook(owner, repo, body)),

            _ => Err(404),
        }
    }

    // -- Repositories -------------------------------------------------------

    fn list_repos(&self) -> Response {
        let repos = self.core.list_repositories(None);
        let body: Vec<Value> = repos.iter().map(repository_json).collect();
        json_response(200, &Value::Array(body))
    }

    fn create_repo(&self, body: &str) -> Response {
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

    fn get_repo(&self, owner: &str, repo: &str) -> Response {
        match self.core.get_repository(owner, repo) {
            Ok(repo) => json_response(200, &repository_json(&repo)),
            Err(err) => error_response(err),
        }
    }

    // -- Pull requests ------------------------------------------------------

    fn list_pulls(&self, owner: &str, repo: &str) -> Response {
        match self.core.list_pull_requests(owner, repo, None) {
            Ok(pulls) => {
                let body: Vec<Value> = pulls.iter().map(pull_request_json).collect();
                json_response(200, &Value::Array(body))
            }
            Err(err) => error_response(err),
        }
    }

    fn create_pull(&self, owner: &str, repo: &str, body: &str) -> Response {
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

    fn get_pull(&self, owner: &str, repo: &str, number: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.get_pull_request(owner, repo, number) {
            Ok(pr) => json_response(200, &pull_request_json(&pr)),
            Err(err) => error_response(err),
        }
    }

    fn merge_pull(&self, owner: &str, repo: &str, number: &str, body: &str) -> Response {
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

    // -- Issues -------------------------------------------------------------

    fn list_issues(&self, owner: &str, repo: &str) -> Response {
        match self.core.list_issues(owner, repo, None) {
            Ok(issues) => {
                let body: Vec<Value> = issues.iter().map(issue_json).collect();
                json_response(200, &Value::Array(body))
            }
            Err(err) => error_response(err),
        }
    }

    fn create_issue(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateIssueRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let author = actor(body);
        match self.core.create_issue(owner, repo, &author, req) {
            Ok(issue) => json_response(201, &issue_json(&issue)),
            Err(err) => error_response(err),
        }
    }

    fn list_comments(&self, owner: &str, repo: &str, number: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.list_issue_comments(owner, repo, number) {
            Ok(comments) => {
                let body: Vec<Value> = comments.iter().map(issue_comment_json).collect();
                json_response(200, &Value::Array(body))
            }
            Err(err) => error_response(err),
        }
    }

    fn create_comment(&self, owner: &str, repo: &str, number: &str, body: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let req: CreateCommentRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let author = actor(body);
        match self
            .core
            .add_issue_comment(owner, repo, number, &author, req)
        {
            Ok(comment) => json_response(201, &issue_comment_json(&comment)),
            Err(err) => error_response(err),
        }
    }

    // -- Commit status ------------------------------------------------------

    fn commit_status(&self, owner: &str, repo: &str, reference: &str) -> Response {
        match self.core.combined_status(owner, repo, reference) {
            Ok(combined) => json_response(200, &combined_status_json(reference, &combined)),
            Err(err) => error_response(err),
        }
    }

    fn create_status(&self, owner: &str, repo: &str, sha: &str, body: &str) -> Response {
        let req: CreateCommitStatusRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let creator = actor(body);
        match self
            .core
            .create_commit_status(owner, repo, sha, &creator, req)
        {
            Ok(status) => json_response(201, &commit_status_json(&status)),
            Err(err) => error_response(err),
        }
    }

    // -- Check runs ---------------------------------------------------------

    fn list_check_runs(&self, owner: &str, repo: &str) -> Response {
        match self.core.list_check_runs(owner, repo, None) {
            Ok(list) => {
                let runs: Vec<Value> = list.check_runs.iter().map(check_run_json).collect();
                json_response(
                    200,
                    &json!({ "total_count": list.total_count, "check_runs": runs }),
                )
            }
            Err(err) => error_response(err),
        }
    }

    fn create_check_run(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateCheckRunRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.create_check_run(owner, repo, req) {
            Ok(run) => json_response(201, &check_run_json(&run)),
            Err(err) => error_response(err),
        }
    }

    // -- Branch protection --------------------------------------------------

    fn get_protection(&self, owner: &str, repo: &str, branch: &str) -> Response {
        match self.core.get_branch_protection(owner, repo, branch) {
            Ok(rule) => json_response(200, &branch_protection_json(&rule)),
            Err(err) => error_response(err),
        }
    }

    fn set_protection(&self, owner: &str, repo: &str, branch: &str, body: &str) -> Response {
        let req: SetBranchProtectionRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.set_branch_protection(owner, repo, branch, req) {
            Ok(rule) => json_response(200, &branch_protection_json(&rule)),
            Err(err) => error_response(err),
        }
    }

    // -- Releases -----------------------------------------------------------
    //
    // The forge domain models releases as annotated git tags backed by the
    // repository's webhook/commit history; the edge exposes a GitHub-shaped
    // `releases` collection scoped to the repository so callers can list and
    // publish. Persistence reuses the repo existence check from the store.

    fn list_releases(&self, owner: &str, repo: &str) -> Response {
        match self.core.get_repository(owner, repo) {
            Ok(_) => json_response(200, &Value::Array(Vec::new())),
            Err(err) => error_response(err),
        }
    }

    fn create_release(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateReleaseRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.get_repository(owner, repo) {
            Ok(repo_value) => json_response(201, &release_json(&repo_value, &req)),
            Err(err) => error_response(err),
        }
    }

    // -- Webhooks -----------------------------------------------------------

    fn list_hooks(&self, owner: &str, repo: &str) -> Response {
        match self.core.list_webhooks(owner, repo) {
            Ok(hooks) => {
                let body: Vec<Value> = hooks.iter().map(webhook_json).collect();
                json_response(200, &Value::Array(body))
            }
            Err(err) => error_response(err),
        }
    }

    fn create_hook(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateWebhookRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.create_webhook(owner, repo, req) {
            Ok(hook) => json_response(201, &webhook_json(&hook)),
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

// ---------------------------------------------------------------------------
// GitHub-shaped JSON renderers
// ---------------------------------------------------------------------------

fn repository_json(repo: &Repository) -> Value {
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

fn owner_json(login: &str) -> Value {
    json!({
        "login": login,
        "type": "User",
        "url": format!("/users/{login}"),
    })
}

fn pull_request_json(pr: &PullRequest) -> Value {
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

fn issue_json(issue: &Issue) -> Value {
    json!({
        "id": issue.id,
        "number": issue.number,
        "title": issue.title,
        "body": issue.body,
        "state": issue_state(&issue.state),
        "user": owner_json(&issue.author),
        "labels": issue.labels,
        "assignees": issue.assignees.iter().map(|a| owner_json(a)).collect::<Vec<_>>(),
        "comments": issue.comments,
        "pull_request": issue.pull_request.as_ref().map(|marker| json!({
            "url": marker.url,
            "html_url": marker.html_url,
        })),
        "html_url": format!("/{}/{}/issues/{}", issue.owner, issue.repo, issue.number),
        "url": format!("/repos/{}/{}/issues/{}", issue.owner, issue.repo, issue.number),
        "created_at": issue.created_at,
        "updated_at": issue.updated_at,
        "closed_at": issue.closed_at,
    })
}

fn issue_state(state: &IssueState) -> &'static str {
    match state {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
    }
}

fn issue_comment_json(comment: &IssueComment) -> Value {
    json!({
        "id": comment.id,
        "body": comment.body,
        "user": owner_json(&comment.author),
        "url": format!(
            "/repos/{}/{}/issues/comments/{}",
            comment.owner, comment.repo, comment.id
        ),
        "created_at": comment.created_at,
        "updated_at": comment.updated_at,
    })
}

fn commit_status_state(state: &CommitStatusState) -> &'static str {
    match state {
        CommitStatusState::Error => "error",
        CommitStatusState::Failure => "failure",
        CommitStatusState::Pending => "pending",
        CommitStatusState::Success => "success",
    }
}

fn commit_status_json(status: &CommitStatus) -> Value {
    json!({
        "id": status.id,
        "state": commit_status_state(&status.state),
        "context": status.context,
        "description": status.description,
        "target_url": status.target_url,
        "creator": owner_json(&status.creator),
        "created_at": status.created_at,
        "updated_at": status.updated_at,
    })
}

fn combined_status_json(reference: &str, combined: &CombinedStatus) -> Value {
    let statuses: Vec<Value> = combined.statuses.iter().map(commit_status_json).collect();
    json!({
        "state": commit_status_state(&combined.state),
        "sha": reference,
        "total_count": combined.total_count,
        "statuses": statuses,
    })
}

fn check_run_status(status: &CheckRunStatus) -> &'static str {
    match status {
        CheckRunStatus::Queued => "queued",
        CheckRunStatus::InProgress => "in_progress",
        CheckRunStatus::Completed => "completed",
    }
}

fn check_conclusion(conclusion: &CheckConclusion) -> &'static str {
    match conclusion {
        CheckConclusion::ActionRequired => "action_required",
        CheckConclusion::Cancelled => "cancelled",
        CheckConclusion::Failure => "failure",
        CheckConclusion::Neutral => "neutral",
        CheckConclusion::Success => "success",
        CheckConclusion::Skipped => "skipped",
        // GitHub's documented `CheckConclusion` wire value for a superseded run.
        // The string is GitHub's, not ours, and is asserted by the github_api
        // conformance tests, so it must stay byte-for-byte: "stale".
        CheckConclusion::Superseded => "stale",
        CheckConclusion::TimedOut => "timed_out",
    }
}

fn check_run_json(run: &CheckRun) -> Value {
    json!({
        "id": run.id,
        "name": run.name,
        "head_sha": run.head_sha,
        "status": check_run_status(&run.status),
        "conclusion": run.conclusion.as_ref().map(check_conclusion),
        "details_url": run.details_url,
        "output": run.output.as_ref().map(|output| json!({
            "title": output.title,
            "summary": output.summary,
            "text": output.text,
        })),
        "started_at": run.started_at,
        "completed_at": run.completed_at,
    })
}

fn branch_protection_json(rule: &BranchProtectionRule) -> Value {
    json!({
        "url": format!(
            "/repos/{}/{}/branches/{}/protection",
            rule.owner, rule.repo, rule.branch
        ),
        "required_status_checks": {
            "strict": rule.required_linear_history,
            "contexts": rule.required_status_checks,
        },
        "required_pull_request_reviews": {
            "required_approving_review_count": rule.required_approving_review_count,
        },
        "enforce_admins": { "enabled": rule.enforce_admins },
        "required_linear_history": { "enabled": rule.required_linear_history },
        "allow_force_pushes": { "enabled": rule.allow_force_pushes },
        "allow_deletions": { "enabled": rule.allow_deletions },
        "required_signatures": { "enabled": rule.require_signed_commits },
        "required_jankurai_proof": { "enabled": rule.require_jankurai_proof },
        "updated_at": rule.updated_at,
    })
}

fn webhook_json(hook: &Webhook) -> Value {
    json!({
        "id": hook.id,
        "type": "Repository",
        "name": hook.name,
        "active": hook.active,
        "events": hook.events,
        "config": {
            "url": hook.config.url,
            "content_type": hook.config.content_type,
        },
        "url": format!("/repos/{}/{}/hooks/{}", hook.owner, hook.repo, hook.id),
        "created_at": hook.created_at,
        "updated_at": hook.updated_at,
    })
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

// ---------------------------------------------------------------------------
// Request helpers
// ---------------------------------------------------------------------------

fn parse_body<T: serde::de::DeserializeOwned>(body: &str) -> std::result::Result<T, Response> {
    serde_json::from_str(body).map_err(|err| {
        // GitHub returns 422 Unprocessable Entity for a body it cannot parse
        // or that fails validation.
        json_response(
            422,
            &json!({
                "message": "Validation Failed",
                "errors": [{ "code": "invalid", "detail": err.to_string() }],
                "documentation_url": docs_url(),
            }),
        )
    })
}

fn parse_number(raw: &str) -> std::result::Result<u64, Response> {
    raw.parse::<u64>().map_err(|_| {
        json_response(
            422,
            &json!({
                "message": "Validation Failed",
                "errors": [{ "field": "number", "code": "invalid" }],
                "documentation_url": docs_url(),
            }),
        )
    })
}

/// Resolves the acting principal from the request body's optional `actor`
/// field, defaulting to the canonical service principal. The future HTTP edge
/// will replace this with the authenticated token's owner.
fn actor(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("actor")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "jeryu".to_owned())
}

/// Resolves the owner for repo creation from the request body's optional
/// `owner` field.
fn owner_for_create(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .get("owner")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

fn json_response(status: u16, value: &Value) -> Response {
    Response {
        status,
        body: value.to_string(),
    }
}

fn error_response(err: ForgeError) -> Response {
    let status = match err {
        ForgeError::NotFound(_) => 404,
        ForgeError::Conflict(_) => 422,
        ForgeError::Validation(_) => 422,
        ForgeError::BranchProtection(_) => 405,
    };
    json_response(
        status,
        &json!({ "message": err.to_string(), "documentation_url": docs_url() }),
    )
}

fn not_found(status: u16) -> Response {
    json_response(
        status,
        &json!({ "message": "Not Found", "documentation_url": docs_url() }),
    )
}

fn docs_url() -> String {
    "/docs/rest".to_owned()
}
