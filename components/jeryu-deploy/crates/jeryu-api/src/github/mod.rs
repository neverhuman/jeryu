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
//!
//! The router itself lives here; the per-resource route handlers and their
//! GitHub-shaped JSON renderers are grouped by resource into sibling
//! submodules ([`repos`], [`pulls`], [`issues`], [`commit_status`],
//! [`check_runs`], [`branch_protection`], [`releases`], [`hooks`]). Shared
//! request parsing and response helpers live in [`support`].

mod actions;
mod branch_protection;
mod check_runs;
mod commit_status;
mod graphql;
mod hooks;
mod issues;
mod pulls;
mod releases;
mod repos;
mod support;

use jeryu_core::ForgeCore;
use jeryu_jira::WorkStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

use crate::routes::Response;

pub(crate) use support::{GH_AUTH_BOUNDARY, GH_SETUP_COMMAND, GH_SETUP_TOKEN_FILE};
#[allow(unused_imports)]
pub(crate) use support::{MCP_GUIDANCE_TOOLS, MCP_RUN_TESTS_TOOL};
use support::{
    Pagination, PullStateSelector, first_contact_response, gh_auth_workaround_response,
    json_response, not_found,
};

/// Semantic version reported by `GET /api/v1/version`.
pub const JERYU_API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// HTTP method understood by the GitHub-compatible edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Patch,
    Post,
    Put,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkBridgeRepair {
    pub code: String,
    pub operation: String,
    pub owner: String,
    pub repo: String,
    pub issue_number: u64,
    pub work_key: Option<String>,
    pub reason: String,
    pub common_fixes: Vec<String>,
    pub docs_url: String,
    pub repair_hint: String,
}

/// GitHub-compatible REST router backed by an in-memory [`ForgeCore`] store.
///
/// When built with the `web` feature, the router may also carry an optional
/// [`jeryu_gitd::RepoManager`] (via [`GithubRouter::with_repo_manager`]). When
/// present, the PR merge endpoint performs a REAL, gated git merge that
/// advances `refs/heads/<base>` in the bare repo; when absent it falls back to
/// the in-memory synthetic-sha merge.
#[derive(Clone, Debug, Default)]
pub struct GithubRouter {
    core: ForgeCore,
    work_store: Option<WorkStore>,
    work_bridge_repairs: Arc<Mutex<Vec<WorkBridgeRepair>>>,
    #[cfg(feature = "web")]
    repo_manager: Option<std::sync::Arc<jeryu_gitd::RepoManager>>,
    #[cfg(feature = "web")]
    github_mirror: Option<std::sync::Arc<crate::github_mirror::GithubMirror>>,
}

impl GithubRouter {
    /// Builds a router over a fresh in-memory forge store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a router over an existing forge store.
    pub fn with_core(core: ForgeCore) -> Self {
        Self {
            core,
            work_store: None,
            work_bridge_repairs: Arc::new(Mutex::new(Vec::new())),
            #[cfg(feature = "web")]
            repo_manager: None,
            #[cfg(feature = "web")]
            github_mirror: None,
        }
    }

    /// Attach the Work Tracker store used by the web server to mirror
    /// user-created GitHub-compatible issues into Work items.
    #[must_use]
    pub fn with_work_store(mut self, work_store: WorkStore) -> Self {
        self.work_store = Some(work_store);
        self
    }

    /// Attach a git [`RepoManager`](jeryu_gitd::RepoManager) so the PR merge
    /// endpoint advances the real base ref in the bare repo. A single additive,
    /// forward-compatible builder call: production wiring chains this onto
    /// [`GithubRouter::with_core`] in `web.rs`.
    #[cfg(feature = "web")]
    #[must_use]
    pub fn with_repo_manager(
        mut self,
        repo_manager: std::sync::Arc<jeryu_gitd::RepoManager>,
    ) -> Self {
        self.repo_manager = Some(repo_manager);
        self
    }

    /// Attach the merge-to-GitHub mirror so a PR merged into a configured
    /// repo's default branch pushes the live main tip to
    /// `github.com/<github_slug>` (outcome recorded as the
    /// `jeryu/github-mirror` check-run; never affects the merge response).
    /// Absent (the default, incl. every test that doesn't opt in) no push is
    /// ever attempted.
    #[cfg(feature = "web")]
    #[must_use]
    pub fn with_github_mirror(
        mut self,
        mirror: std::sync::Arc<crate::github_mirror::GithubMirror>,
    ) -> Self {
        self.github_mirror = Some(mirror);
        self
    }

    /// Borrows the backing forge store (used by tests and embedding callers).
    pub fn core(&self) -> &ForgeCore {
        &self.core
    }

    pub fn work_bridge_repairs(&self) -> Vec<WorkBridgeRepair> {
        self.work_bridge_repairs
            .lock()
            .expect("work bridge repair queue lock")
            .clone()
    }

    fn record_work_bridge_repair(&self, repair: WorkBridgeRepair) {
        self.work_bridge_repairs
            .lock()
            .expect("work bridge repair queue lock")
            .push(repair);
    }

    /// Dispatches a request. `body` is the raw JSON request body (empty for
    /// bodiless GETs). The actor is the authenticated principal; the in-memory
    /// edge defaults it where GitHub would take it from the token.
    pub fn handle(&self, method: Method, path: &str, body: &str) -> Response {
        let path = normalize_github_path(path);
        // Split a `path?query` so callers (tests, the future HTTP edge) can pass
        // RFC5988 list pagination as `?per_page=&page=` without the query
        // leaking into segment matching.
        let (route_path, query) = path.split_once('?').unwrap_or((path, ""));
        let page = Pagination::from_query(query);
        let pull_state = PullStateSelector::from_query(query);
        let segments: Vec<&str> = route_path.trim_matches('/').split('/').collect();
        self.route(method, &segments, body, route_path, page, pull_state)
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
        path: &str,
        page: Pagination,
        pull_state: PullStateSelector,
    ) -> std::result::Result<Response, u16> {
        use Method::{Get, Patch, Post, Put};
        match (method, segments) {
            (Get, ["health"]) => Ok(json_response(
                200,
                &json!({ "status": "ok", "service": "jeryu-api" }),
            )),
            // Steering: first-contact doc for a confused agent on the REST edge.
            (Get, [".jeryu", "agents", "first-contact"]) => Ok(first_contact_response()),
            (
                _,
                ["login", "device", "code"]
                | ["login", "oauth", "access_token"]
                | ["login", "oauth", "authorize"],
            ) => Ok(gh_auth_workaround_response(path)),
            (Get, ["api", "v1", "version"]) => Ok(json_response(
                200,
                &json!({ "version": JERYU_API_VERSION, "name": "jeryu-api" }),
            )),
            (Get, ["user"]) => Ok(json_response(
                200,
                &json!({
                    "login": "jeryu",
                    "id": 1,
                    "node_id": "U_jeryu",
                    "type": "User",
                    "name": "Jeryu Local Operator",
                    "url": "/user",
                }),
            )),
            (Post, ["graphql"]) => Ok(self.graphql(body)),

            // Repositories ---------------------------------------------------
            (Get, ["repos"]) => Ok(self.list_repos(path, page)),
            (Post, ["repos"]) => Ok(self.create_repo(body)),
            (Get, ["repos", owner, repo]) => Ok(self.get_repo(owner, repo)),

            // Pull requests --------------------------------------------------
            (Get, ["repos", owner, repo, "pulls"]) => {
                Ok(self.list_pulls(owner, repo, path, page, pull_state))
            }
            (Post, ["repos", owner, repo, "pulls"]) => Ok(self.create_pull(owner, repo, body)),
            (Get, ["repos", owner, repo, "pulls", number]) => {
                Ok(self.get_pull(owner, repo, number))
            }
            (Patch, ["repos", owner, repo, "pulls", number]) => {
                Ok(self.update_pull(owner, repo, number, body))
            }
            (Put, ["repos", owner, repo, "pulls", number, "merge"]) => {
                Ok(self.merge_pull(owner, repo, number, body))
            }

            // Issues ---------------------------------------------------------
            (Get, ["repos", owner, repo, "issues"]) => {
                Ok(self.list_issues(owner, repo, path, page))
            }
            (Post, ["repos", owner, repo, "issues"]) => Ok(self.create_issue(owner, repo, body)),
            (Get, ["repos", owner, repo, "issues", number]) => {
                Ok(self.get_issue(owner, repo, number))
            }
            (Patch, ["repos", owner, repo, "issues", number]) => {
                Ok(self.update_issue(owner, repo, number, body))
            }
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
            (Get, ["repos", owner, repo, "check-runs"]) => {
                Ok(self.list_check_runs(owner, repo, path, page))
            }
            (Get, ["repos", owner, repo, "commits", reference, "check-runs"]) => {
                Ok(self.list_check_runs_for_reference(owner, repo, reference, path, page))
            }
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
            (Get, ["repos", owner, repo, "releases"]) => {
                Ok(self.list_releases(owner, repo, path, page))
            }
            (Post, ["repos", owner, repo, "releases"]) => {
                Ok(self.create_release(owner, repo, body))
            }

            // Actions (sourced from check-runs as a CI proxy) ----------------
            (Get, ["repos", owner, repo, "actions", "runs"]) => {
                Ok(self.list_action_runs(owner, repo, path, page))
            }
            (Get, ["repos", owner, repo, "actions", "runs", id]) => {
                Ok(self.get_action_run(owner, repo, id))
            }
            (Get, ["repos", owner, repo, "actions", "runs", id, "jobs"]) => {
                Ok(self.list_action_run_jobs(owner, repo, id))
            }
            (Get, ["repos", owner, repo, "actions", "workflows"]) => {
                Ok(self.list_action_workflows(owner, repo, path, page))
            }
            (Get, ["repos", owner, repo, "actions", "workflows", workflow_id]) => {
                Ok(self.get_action_workflow(owner, repo, workflow_id))
            }
            (
                Get,
                [
                    "repos",
                    owner,
                    repo,
                    "actions",
                    "workflows",
                    workflow_id,
                    "runs",
                ],
            ) => Ok(self.list_action_workflow_runs(owner, repo, workflow_id, path, page)),
            (Post, ["repos", owner, repo, "actions", ..]) => {
                Ok(self.unsupported_action_write(owner, repo))
            }

            // Webhooks -------------------------------------------------------
            (Get, ["repos", owner, repo, "hooks"]) => Ok(self.list_hooks(owner, repo)),
            (Post, ["repos", owner, repo, "hooks"]) => Ok(self.create_hook(owner, repo, body)),

            _ => Err(404),
        }
    }
}

fn normalize_github_path(path: &str) -> &str {
    let Some(rest) = path.strip_prefix("/api/v3") else {
        return path;
    };
    if rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') {
        if rest.is_empty() || rest.starts_with('?') {
            "/"
        } else {
            rest
        }
    } else {
        path
    }
}
