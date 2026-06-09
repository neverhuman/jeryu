//! Shared request parsing, principal resolution, and response helpers for the
//! GitHub-compatible edge.
//!
//! These helpers are used across every resource submodule so they live in one
//! place; the GitHub-shaped status codes (422 for unparseable/invalid bodies,
//! 405 for branch-protection blocks, 404 for misses) are asserted by the
//! `github_api` conformance tests and must stay byte-for-byte.

use jeryu_core::ForgeError;
use serde_json::{Value, json};

use crate::routes::Response;

pub(crate) const MCP_GUIDANCE_TOOLS: &[&str] = &[
    "jeryu.get_system_snapshot",
    "jeryu.get_ci_run_jobs",
    "jeryu.explain_blockers",
    "jeryu.run_tests",
    "jeryu.plan_validation",
    "jeryu.propose_patch",
    "jeryu.request_merge",
    "jeryu.bug_submit",
    "jeryu.bug_list",
    "jeryu.workcell.claim",
    "jeryu.workcell.status",
    "jeryu.workcell.repair_live",
    "jeryu.workcell.export_pr",
    "jeryu.workcell.release",
];

pub(crate) const MCP_RUN_TESTS_TOOL: &str = "jeryu.run_tests";

/// The fast-path pointer surfaced on every error body so a confused agent is
/// always handed the capability manifest instead of being left to guess.
pub(super) const FASTER_PATH: &str = "/.jeryu/capabilities";

/// RFC 5988 list pagination parsed off the request query string. GitHub's
/// defaults (`per_page=30`, `page=1`) and ceiling (`per_page<=100`) are
/// mirrored so `gh ... --paginate` and naive `?page=N` walks behave.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pagination {
    pub(super) per_page: usize,
    pub(super) page: usize,
}

impl Pagination {
    pub(super) const DEFAULT_PER_PAGE: usize = 30;
    pub(super) const MAX_PER_PAGE: usize = 100;

    /// Parses `?per_page=&page=` from a raw query string (the part after `?`).
    /// Out-of-range or unparseable values fall back to the GitHub defaults so a
    /// malformed page hint never errors a list route.
    pub(super) fn from_query(query: &str) -> Self {
        let mut per_page = Self::DEFAULT_PER_PAGE;
        let mut page = 1usize;
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "per_page" => {
                    if let Ok(parsed) = value.parse::<usize>() {
                        per_page = parsed.clamp(1, Self::MAX_PER_PAGE);
                    }
                }
                "page" => {
                    if let Ok(parsed) = value.parse::<usize>()
                        && parsed >= 1
                    {
                        page = parsed;
                    }
                }
                _ => {}
            }
        }
        Self { per_page, page }
    }
}

/// GitHub's `?state=` selector for the pulls list endpoint. GitHub renders a PR
/// as either `open` or `closed` (a merged PR is a `closed` sub-state with
/// `merged_at` set), so the three selectors map onto that rendered state:
/// `open` keeps open PRs, `closed` keeps closed/merged PRs, and `all` keeps
/// everything. Absent or unrecognized values fall back to GitHub's documented
/// default (`open`) rather than erroring the route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PullStateSelector {
    Open,
    Closed,
    All,
}

impl PullStateSelector {
    /// Parses `?state=` off a raw query string. The first `state` pair wins; an
    /// absent param or any value other than `closed`/`all` yields `Open` so a
    /// stray hint matches GitHub's default instead of 500-ing.
    pub(super) fn from_query(query: &str) -> Self {
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            if key == "state" {
                return match value {
                    "all" => Self::All,
                    "closed" => Self::Closed,
                    _ => Self::Open,
                };
            }
        }
        Self::Open
    }

    /// Whether a PR whose GitHub-rendered `state` field is `github_state`
    /// (`open` or `closed`) belongs in the response for this selector.
    pub(super) fn keeps(self, github_state: &str) -> bool {
        match self {
            Self::All => true,
            Self::Open => github_state == "open",
            Self::Closed => github_state == "closed",
        }
    }
}

/// Slices `items` to the requested page and renders a GitHub-shaped 200 list
/// response carrying an RFC 5988 `Link` header (`next`/`last`/`prev`/`first`)
/// when more than one page exists. `render` shapes the JSON body from the page
/// slice (a bare array for most lists; a wrapped object for check-runs/actions).
pub(super) fn paginate<T, F>(
    base_path: &str,
    page_args: Pagination,
    items: &[T],
    render: F,
) -> Response
where
    F: FnOnce(&[T], usize) -> Value,
{
    let total = items.len();
    let per_page = page_args.per_page;
    let last_page = total.div_ceil(per_page).max(1);
    let page = page_args.page;
    let start = page.saturating_sub(1).saturating_mul(per_page);
    let slice: &[T] = if start >= total {
        &[]
    } else {
        let end = (start + per_page).min(total);
        &items[start..end]
    };
    let body = render(slice, total);
    let mut headers = Vec::new();
    if let Some(link) = link_header(base_path, per_page, page, last_page) {
        headers.push(("Link".to_owned(), link));
    }
    Response {
        status: 200,
        body: body.to_string(),
        headers,
    }
}

/// Builds the RFC 5988 `Link` header value. Emits `next`/`last` while more
/// pages remain and `prev`/`first` once past page 1; returns `None` for a
/// single-page result (GitHub omits the header entirely in that case).
fn link_header(base_path: &str, per_page: usize, page: usize, last_page: usize) -> Option<String> {
    if last_page <= 1 {
        return None;
    }
    let url = |target: usize| format!("{base_path}?per_page={per_page}&page={target}");
    let mut parts: Vec<String> = Vec::new();
    if page < last_page {
        parts.push(format!("<{}>; rel=\"next\"", url(page + 1)));
        parts.push(format!("<{}>; rel=\"last\"", url(last_page)));
    }
    if page > 1 {
        parts.push(format!("<{}>; rel=\"prev\"", url(page - 1)));
        parts.push(format!("<{}>; rel=\"first\"", url(1)));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// The jeryu steering block stamped onto every error body: a faster path, the
/// MCP tool that would have served the intent, and a one-line hint. Errors
/// teach instead of dead-ending the agent.
pub(super) fn steering(mcp_tool: &str, hint: &str) -> Value {
    json!({
        "faster_path": FASTER_PATH,
        "mcp_tool": mcp_tool,
        "hint": hint,
    })
}

pub(super) fn actions_write_steering() -> Value {
    json!({
        "faster_path": FASTER_PATH,
        "mcp_tool": MCP_RUN_TESTS_TOOL,
        "mcp_tools": [MCP_RUN_TESTS_TOOL, "jeryu.get_ci_run_jobs"],
        "hint": "use jeryu.run_tests for the local CI action and jeryu.get_ci_run_jobs to inspect existing runs and workflows",
    })
}

pub(super) fn parse_body<T: serde::de::DeserializeOwned>(
    body: &str,
) -> std::result::Result<T, Response> {
    serde_json::from_str(body).map_err(|err| {
        // GitHub returns 422 Unprocessable Entity for a body it cannot parse
        // or that fails validation.
        json_response(
            422,
            &json!({
                "message": "Validation Failed",
                "errors": [{ "code": "invalid", "detail": err.to_string() }],
                "documentation_url": docs_url(),
                "jeryu_steering": steering(
                    "jeryu.propose_patch",
                    "the JSON body did not validate; the typed jeryu MCP tools build a valid request for you",
                ),
            }),
        )
    })
}

pub(super) fn parse_number(raw: &str) -> std::result::Result<u64, Response> {
    raw.parse::<u64>().map_err(|_| {
        json_response(
            422,
            &json!({
                "message": "Validation Failed",
                "errors": [{ "field": "number", "code": "invalid" }],
                "documentation_url": docs_url(),
                "jeryu_steering": steering(
                    "jeryu.explain_blockers",
                    "the path expected a numeric id; list the resource first to get a valid number",
                ),
            }),
        )
    })
}

/// Resolves the acting principal from the request body's optional `actor`
/// field, defaulting to the canonical service principal. The future HTTP edge
/// will replace this with the authenticated token's owner.
pub(super) fn actor(body: &str) -> String {
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
pub(super) fn owner_for_create(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body).ok().and_then(|value| {
        value
            .get("owner")
            .and_then(Value::as_str)
            .map(str::to_owned)
    })
}

/// Renders an `owner`/`user` actor block shared by every GitHub-shaped entity.
pub(super) fn owner_json(login: &str) -> Value {
    json!({
        "login": login,
        "type": "User",
        "url": format!("/users/{login}"),
    })
}

pub(super) fn json_response(status: u16, value: &Value) -> Response {
    Response {
        status,
        body: value.to_string(),
        headers: Vec::new(),
    }
}

/// Like [`json_response`] but attaches advisory response headers. Used by the
/// overlap engine to stamp `X-Jeryu-Reused-PR` when a create-PR request is
/// routed onto an existing open PR.
pub(super) fn json_response_with_headers(
    status: u16,
    value: &Value,
    headers: Vec<(String, String)>,
) -> Response {
    Response {
        status,
        body: value.to_string(),
        headers,
    }
}

pub(super) fn error_response(err: ForgeError) -> Response {
    let (status, mcp_tool, hint) = match err {
        ForgeError::NotFound(_) => (
            404,
            "jeryu.get_system_snapshot",
            "the resource was not found; snapshot the system to find live owners/repos/numbers",
        ),
        ForgeError::Conflict(_) => (
            422,
            "jeryu.propose_patch",
            "the request conflicts with current state; let the typed MCP tool reconcile it",
        ),
        ForgeError::Validation(_) => (
            422,
            "jeryu.propose_patch",
            "the request failed validation; the typed MCP tool builds a valid request for you",
        ),
        ForgeError::BranchProtection(_) => (
            405,
            "jeryu.explain_blockers",
            "branch protection blocks this; ask jeryu to explain the blockers and required checks",
        ),
        ForgeError::Storage(_) => (
            500,
            "jeryu.get_system_snapshot",
            "an internal store error occurred; snapshot the system and retry",
        ),
    };
    json_response(
        status,
        &json!({
            "message": err.to_string(),
            "documentation_url": docs_url(),
            "jeryu_steering": steering(mcp_tool, hint),
        }),
    )
}

pub(super) fn actions_write_response(owner: &str, repo: &str) -> Response {
    json_response(
        501,
        &json!({
            "message": "Hosted GitHub Actions writes are not supported on Jeryu; CI is local and MCP-driven.",
            "documentation_url": docs_url(),
            "jeryu_repair_hint": {
                "purpose": "route unsupported GitHub Actions write request",
                "reason": "Jeryu intentionally supports local MCP-driven CI and guided read surfaces instead of hosted Actions dispatch, rerun, or cancel writes.",
                "common_fixes": [
                    "use /.jeryu/capabilities to choose the local MCP path for the CI action",
                    "use jeryu.run_tests instead of hosted Actions writes to run the local CI flow",
                    "use GET /repos/{owner}/{repo}/actions/runs, GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}, or jeryu.get_ci_run_jobs to inspect existing runs before retrying"
                ],
                "docs_url": docs_url(),
                "repair_hint": "use jeryu.run_tests for the CI action, then rerun cargo test -p jeryu-api --features web --jobs 40",
            },
            "jeryu_connection": {
                "capabilities": FASTER_PATH,
                "first_contact": "/.jeryu/agents/first-contact",
                "mcp": "/mcp",
                "actions_runs": format!("GET /repos/{owner}/{repo}/actions/runs"),
                "actions_run": format!("GET /repos/{owner}/{repo}/actions/runs/{{id}}"),
                "actions_run_jobs": format!("GET /repos/{owner}/{repo}/actions/runs/{{id}}/jobs"),
                "actions_workflows": format!("GET /repos/{owner}/{repo}/actions/workflows"),
                "actions_workflow": format!("GET /repos/{owner}/{repo}/actions/workflows/{{workflow_id}}"),
                "actions_workflow_runs": format!("GET /repos/{owner}/{repo}/actions/workflows/{{workflow_id}}/runs"),
            },
            "jeryu_mcp_tools": MCP_GUIDANCE_TOOLS,
            "jeryu_steering": actions_write_steering(),
        }),
    )
}

pub(super) fn gh_auth_workaround_response(path: &str) -> Response {
    json_response(
        501,
        &json!({
            "message": "Direct GitHub CLI auth setup is not supported for a Jeryu host.",
            "documentation_url": "docs/errors.md#github-cli-auth-steering",
            "jeryu_repair_hint": {
                "purpose": "route GitHub CLI auth setup through Jeryu",
                "reason": "Jeryu uses an explicit gh hosts.yml entry plus portable agent-auth receipts; running gh auth login, refresh, or token-hunting workarounds against the Jeryu host is the wrong repair path.",
                "common_fixes": [
                    "stop running gh auth login, gh auth refresh, or credential-store searches for the Jeryu host",
                    "run jeryu gh-setup --host http://127.0.0.1:8787 --token JERYU-TOKEN, or rerun it with the live JERYU_API_URL",
                    "use GET /.jeryu/capabilities or the listed jeryu.* MCP tools for PR, CI, issue, and repo workflows",
                    "for agent native CLI credentials, run jeryu agent auth doctor <tool> and jeryu agent auth import --from-host <tool>"
                ],
                "docs_url": "docs/errors.md#github-cli-auth-steering",
                "repair_hint": "rerun jeryu gh-setup for the Jeryu host, then retry the original Jeryu CLI/MCP/API operation instead of gh auth"
            },
            "jeryu_connection": {
                "capabilities": FASTER_PATH,
                "first_contact": "/.jeryu/agents/first-contact",
                "mcp": "/mcp",
                "gh_setup": "jeryu gh-setup --host http://127.0.0.1:8787 --token JERYU-TOKEN",
                "agent_auth_doctor": "jeryu agent auth doctor <codex|claude|jekko>",
                "agent_auth_import": "jeryu agent auth import --from-host <codex|claude|jekko>"
            },
            "jeryu_mcp_tools": MCP_GUIDANCE_TOOLS,
            "jeryu_steering": steering(
                "jeryu.get_system_snapshot",
                "stop the gh auth flow; configure gh with jeryu gh-setup and use Jeryu MCP/API routes for the original task",
            ),
            "path": path,
        }),
    )
}

pub(super) fn not_found(status: u16) -> Response {
    json_response(
        status,
        &json!({
            "message": "Not Found",
            "documentation_url": docs_url(),
            "jeryu_repair_hint": {
                "purpose": "route unsupported GitHub-compatible REST request",
                "reason": "the request path is outside the current guided Jeryu GitHub subset",
                "common_fixes": [
                    "retry with one of the listed GitHub-compatible REST routes",
                    "use the typed Jeryu MCP/API tool for repository, PR, issue, check, release, or hook workflows",
                    "add a conformance test before widening the compatibility subset"
                ],
                "docs_url": docs_url(),
                "repair_hint": "map the command to the closest listed Jeryu route or MCP tool, then rerun cargo test -p jeryu-api --features web"
            },
            "jeryu_mcp_tools": MCP_GUIDANCE_TOOLS,
            "jeryu_steering": steering(
                "jeryu.get_system_snapshot",
                "this path is outside the guided subset; GET /.jeryu/capabilities and prefer the MCP tools",
            ),
            "jeryu_api_routes": [
                "GET /user",
                "GET /repos",
                "GET /repos/{owner}/{repo}",
                "GET /repos/{owner}/{repo}/pulls",
                "GET /repos/{owner}/{repo}/issues",
                "GET /repos/{owner}/{repo}/commits/{ref}/status",
                "GET /repos/{owner}/{repo}/commits/{ref}/check-runs",
                "GET /repos/{owner}/{repo}/actions/runs",
                "GET /repos/{owner}/{repo}/actions/runs/{id}",
                "GET /repos/{owner}/{repo}/actions/runs/{id}/jobs",
                "GET /repos/{owner}/{repo}/actions/workflows",
                "GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}",
                "GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}/runs",
                "POST /graphql"
            ]
        }),
    )
}

pub(super) fn docs_url() -> String {
    "/docs/rest".to_owned()
}

/// First-contact doc for a confused agent that landed on the REST edge. Points
/// it at the capability manifest and the typed MCP tools instead of letting it
/// brute-force bespoke `gh` invocations.
pub(super) fn first_contact_response() -> Response {
    json_response(
        200,
        &json!({
            "message": "Welcome — you are talking to the Jeryu GitHub-compatible edge.",
            "start_here": FASTER_PATH,
            "advice": [
                "GET /.jeryu/capabilities for the live endpoint + gh-command map.",
                "Prefer the typed jeryu.* MCP tools over bespoke gh REST calls; they are faster and never dead-end.",
                "Do not run gh auth login or gh auth refresh for a Jeryu host; run jeryu gh-setup for the host entry.",
                "Every error reply carries a jeryu_steering block naming the MCP tool that serves your intent.",
            ],
            "gh_auth_policy": {
                "do_not_run": ["gh auth login", "gh auth refresh", "credential-store token hunting"],
                "run_instead": "jeryu gh-setup --host http://127.0.0.1:8787 --token JERYU-TOKEN",
                "agent_auth": "jeryu agent auth doctor <tool>; jeryu agent auth import --from-host <tool>",
            },
            "mcp_tools": MCP_GUIDANCE_TOOLS,
            "documentation_url": docs_url(),
        }),
    )
}
