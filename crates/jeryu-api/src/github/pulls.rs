//! Pull request routes (`/repos/{owner}/{repo}/pulls...`) and their
//! GitHub-shaped renderers.

use jeryu_core::{
    ChangeSet, CreatePullRequestRequest, ForgeError, MergePullRequestRequest, OpenPr,
    OverlapConfig, OverlapDecision, PullRequest, PullRequestState, decide,
};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{
    actor, docs_url, error_response, json_response, json_response_with_headers, owner_json,
    parse_body, parse_number,
};

/// Response header stamped when a create-PR request is hot-fixed onto an
/// existing open PR instead of opening a fresh one.
const HDR_REUSED_PR: &str = "X-Jeryu-Reused-PR";

/// The base SHA the forge assigns when a create request omits `base_sha`.
/// Mirrored here so the overlap engine compares the proposed change against
/// existing PRs on the same default base (see `ForgeCore::create_pull_request`).
const DEFAULT_BASE_SHA: &str = "base";

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

        // Flagship overlap routing: before opening a fresh PR, see whether the
        // proposed change overlaps an existing OPEN PR enough to hot-fix it.
        // Only runs when the request carries `changed_files`; without them there
        // is nothing to score, so we fall through to a normal create.
        if !req.changed_files.is_empty()
            && let Some(response) = self.maybe_route_overlap(owner, repo, &author, &req)
        {
            return response;
        }

        match self.core.create_pull_request(owner, repo, &author, req) {
            Ok(pr) => json_response(201, &pull_request_json(&pr)),
            Err(err) => error_response(err),
        }
    }

    /// Runs the PR-overlap engine for a proposed change. Returns:
    /// * `Some(route_to_existing 200)` with an `X-Jeryu-Reused-PR` header when
    ///   the change should hot-fix an existing open PR,
    /// * `Some(409)` when the best candidate overlaps but coalescing is unsafe
    ///   (stale base / unproven head),
    /// * `None` when a fresh PR should be created (caller proceeds as normal).
    ///
    /// Any failure to list the repo's open PRs is treated as "no candidates"
    /// (returns `None`) so overlap routing can never block a legitimate create.
    fn maybe_route_overlap(
        &self,
        owner: &str,
        repo: &str,
        author: &str,
        req: &CreatePullRequestRequest,
    ) -> Option<Response> {
        // List every PR and keep the ones GitHub would render as `open`. We do
        // NOT filter by `PullRequestState::Open` at the engine: a healthy PR is
        // re-evaluated to a richer lifecycle state (e.g. `Mergeable`) on read,
        // so an exact-`Open` filter would miss live candidates. Only terminal
        // Merged/Closed PRs are excluded.
        let open_prs = self.core.list_pull_requests(owner, repo, None).ok()?;

        let open: Vec<OpenPr> = open_prs
            .iter()
            .filter(|pr| {
                !matches!(
                    pr.state,
                    PullRequestState::Merged | PullRequestState::Closed
                ) && !pr.merged
            })
            .map(|pr| {
                OpenPr::new(
                    pr.number,
                    pr.changed_files.clone(),
                    pr.base.sha.clone(),
                    // A PR is only safe to coalesce onto if its head currently
                    // evaluates as mergeable (checks/protection green).
                    pr.mergeable,
                )
            })
            .collect();

        if open.is_empty() {
            return None;
        }

        let base_sha = req
            .base_sha
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_SHA.to_string());
        let change = ChangeSet::new(
            req.changed_files.clone(),
            base_sha,
            Some(author.to_string()),
        );

        match decide(&change, &open, OverlapConfig::default()) {
            OverlapDecision::RouteToExisting { pr, reason } => {
                let payload = json!({
                    "route_to_existing": {
                        "pr": pr,
                        "reason": reason,
                    },
                    "message": format!(
                        "change coalesced onto existing pull request #{pr}; no new PR created"
                    ),
                    "documentation_url": docs_url(),
                });
                Some(json_response_with_headers(
                    200,
                    &payload,
                    vec![(HDR_REUSED_PR.to_string(), pr.to_string())],
                ))
            }
            OverlapDecision::RefuseCoalesce { pr, reason } => {
                // GitHub returns 409 Conflict when a change cannot be applied
                // cleanly onto its target; the overlap engine refuses to clobber
                // a stale base or stack work on an unproven head.
                let payload = json!({
                    "message": reason,
                    "refuse_coalesce": { "pr": pr },
                    "documentation_url": docs_url(),
                });
                Some(json_response(409, &payload))
            }
            // CreateNew: nothing safe to coalesce onto; proceed with a fresh PR.
            OverlapDecision::CreateNew { .. } => None,
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
