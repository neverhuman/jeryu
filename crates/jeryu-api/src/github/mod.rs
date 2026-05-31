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

mod branch_protection;
mod check_runs;
mod commit_status;
mod hooks;
mod issues;
mod pulls;
mod releases;
mod repos;
mod support;

use jeryu_core::ForgeCore;
use serde_json::json;

use crate::routes::Response;

use support::{json_response, not_found};

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

    /// Builds a router over an existing forge store.
    pub fn with_core(core: ForgeCore) -> Self {
        Self { core }
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
}
