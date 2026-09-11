//! HTTP router and leaf handlers extracted from the web edge.
use std::path::Path;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Extension, Path as AxumPath, Request, State};
use axum::http::{HeaderName, HeaderValue, Method as HttpMethod, StatusCode, header};
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::{any, get, post};
use axum::{Json, Router as AxumRouter};
use jeryu_core::AccountSummary;
use jeryu_readmodel::TuiReadModel;
use serde_json::{Value, json};

use super::*;

pub(super) fn app(state: WebState, spa_dir: &Path) -> AxumRouter {
    let mut state = state;
    state.spa_dir = spa_dir.to_path_buf();
    let state = Arc::new(state);
    let mcp_state = Arc::new(jeryu_mcp::McpHttpState::new(Arc::new(
        mcp_backend::WebMcpBackend::new(state.clone()),
    )));
    let mcp_router = jeryu_mcp::mcp_router(mcp_state)
        .layer(from_fn(steer_headers))
        .layer(from_fn_with_state(state.clone(), auth::gate))
        .layer(from_fn(request_id::propagate));
    AxumRouter::new()
        .route("/health", get(health))
        // Steering surface: advertises the faster jeryu/MCP path so external
        // agents stuck on bespoke `gh` commands can discover it.
        .route("/.jeryu/capabilities", get(capabilities))
        .route("/api/v1/bootstrap", get(bootstrap))
        .route("/api/v1/bootstrap.tui", get(bootstrap_tui))
        .route("/api/v1/work", get(work::list).post(work::create))
        .route("/api/v1/work/:key", get(work::detail).patch(work::patch))
        .route("/api/v1/work/:key/comments", post(work::comment))
        .route("/api/v1/work/:key/links", post(work::link))
        .route("/api/v1/auth/signup", post(auth::signup))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/api/v1/auth/password", post(auth::change_password))
        .route(
            "/api/v1/auth/tokens",
            get(auth::list_tokens).post(auth::create_token),
        )
        .route(
            "/api/v1/auth/tokens/:id",
            axum::routing::delete(auth::delete_token),
        )
        .route("/api/v1/admin/users", get(auth::admin_users))
        .route(
            "/api/v1/admin/users/:login/reset-password",
            post(auth::admin_reset_password),
        )
        .route(
            "/api/v1/admin/repos/:owner/:repo/grants",
            get(auth::admin_repo_grants),
        )
        .route(
            "/api/v1/admin/repos/:owner/:repo/grants/:login",
            post(auth::admin_grant_repo).delete(auth::admin_revoke_repo),
        )
        .route(
            "/api/v1/agent-runs",
            get(agent_runs::list).post(agent_runs::start),
        )
        .route("/api/v1/agent-runs/:id", get(agent_runs::status))
        .route("/api/v1/agent-runs/:id/events", get(agent_runs::events))
        // Live raw-TTY push transport (Server-Sent Events). An outside service such
        // as jpmc subscribes once and is streamed raw bytes as they publish, instead
        // of cursor-polling agent_work.tail; it replays the retained ring on connect.
        .route(
            "/api/v1/agent-runs/:id/tty/stream",
            get(agent_runs::tty_stream),
        )
        .route("/api/v1/agent-runs/:id/control", post(agent_runs::control))
        .route("/api/v1/agent-runs/:id/shell", post(agent_runs::shell))
        .route(
            "/api/v1/agent-runs/:id/export_pr",
            post(agent_runs::export_pr),
        )
        // Host-mediated publish: advance the session branch ref + open a PR. The
        // agent never pushes; the ref move goes through the protected ref service.
        .route("/api/v1/agent-runs/:id/publish", post(sessions::publish))
        .route(
            "/api/v1/workcells",
            get(workcells::list).post(workcells::claim),
        )
        .route(
            "/api/v1/workcells/repair_live",
            post(workcells::repair_live),
        )
        .route("/api/v1/workcells/:id", get(workcells::status))
        .route(
            "/api/v1/workcells/:id/heartbeat",
            post(workcells::heartbeat),
        )
        .route("/api/v1/workcells/:id/release", post(workcells::release))
        .route(
            "/api/v1/workcells/:id/run_agent",
            post(workcells::run_agent),
        )
        .route(
            "/api/v1/workcells/:id/export_pr",
            post(workcells::export_pr),
        )
        .route("/api/v1/repos", get(repos).post(repository_create::create))
        .route("/api/v1/repos/preview", post(repository_create::preview))
        .route(
            "/api/v1/repos/:id",
            get(repo_detail)
                .patch(repo_update)
                .delete(repo_admin::repo_delete),
        )
        // Repo-scoped agent sessions: launch a hardened session, and the live
        // per-repo agent-runs list the web Active-Agents page consumes.
        .route("/api/v1/repos/:id/sessions", post(sessions::create))
        .route("/api/v1/repos/:id/agent-runs", get(sessions::list))
        .route(
            "/api/v1/repos/:id/work",
            get(work::repo_list).post(work::repo_create),
        )
        .route("/api/v1/repos/:id/pulls", get(pulls::list))
        .route("/api/v1/repos/:id/pulls/:number", get(pulls::detail))
        .route("/api/v1/repos/:id/pulls/:number/diff", get(pulls::diff))
        .route("/api/v1/repos/:id/pulls/:number/checks", get(pulls::checks))
        .route(
            "/api/v1/repos/:id/pulls/:number/threads",
            get(pulls::threads),
        )
        .route(
            "/api/v1/repos/:id/pulls/:number/reviews",
            get(pulls::review_history).post(pulls::review),
        )
        .route(
            "/api/v1/repos/:id/pulls/:number/reviews/:review_id/dismiss",
            post(pulls::dismiss_review),
        )
        .route(
            "/api/v1/repos/:id/pulls/:number/comments",
            post(pulls::comment),
        )
        .route(
            "/api/v1/repos/:id/pulls/:number/approve",
            post(pulls::approve),
        )
        .route("/api/v1/repos/:id/pulls/:number/merge", post(pulls::merge))
        .route(
            "/api/v1/repos/:id/jankurai-scores",
            get(repo_jankurai_scores_list).post(repo_jankurai_scores_ingest),
        )
        .route("/api/v1/fleet/tool-adoption", get(fleet_tool_adoption))
        .route(
            "/api/v1/tools/registry/summary",
            get(tool_registry::summary),
        )
        .route("/api/v1/repos/:id/refs", get(repo_refs))
        .route("/api/v1/repos/:id/tree", get(repo_tree))
        .route("/api/v1/repos/:id/blob", get(repo_blob))
        .route("/api/v1/repos/:id/raw", get(repo_raw))
        .route("/api/v1/repos/:id/codegraph/query", post(codegraph::query))
        .route(
            "/api/v1/codegraph/tool-build/status",
            get(tool_build::status),
        )
        .route(
            "/api/v1/codegraph/tool-build/clusters",
            get(tool_build::clusters),
        )
        .route(
            "/api/v1/codegraph/tool-build/clusters/:id/feedback",
            post(tool_build::feedback),
        )
        // System-wide tool-finder: live scan trigger/status, the /tools
        // pattern-family dashboard, and cluster -> registry proposal.
        .route(
            "/api/v1/tool-finder/scan",
            get(tool_finder::scan_status).post(tool_finder::scan_start),
        )
        .route("/api/v1/tool-finder/dashboard", get(tool_finder::dashboard))
        .route(
            "/api/v1/tool-finder/propose/:cluster_id",
            post(tool_finder::propose),
        )
        .route("/api/v1/control-plane/status", get(control_plane::status))
        .route(
            "/api/v1/control-plane/priorities",
            get(control_plane::priorities),
        )
        .route(
            "/api/v1/control-plane/repo-graph",
            get(control_plane::repo_graph),
        )
        .route(
            "/api/v1/control-plane/artifacts/latest",
            get(control_plane::artifacts_latest),
        )
        .route("/api/v1/control-plane/runners", get(control_plane::runners))
        .route(
            "/api/v1/repos/:id/readme",
            get(repo_readme).put(repo_readme_update),
        )
        // Read-only ecosystem surface for generic external clients: the live
        // tool-graph and per-CI-run evidence. Additive, never mutating.
        .route("/api/v1/ecosystem", get(ecosystem))
        .route("/api/v1/ci/runs/:id/evidence", get(ci_run_evidence))
        .route("/api/v1/markdown/render", post(markdown_render))
        .route("/api/v1/ws", get(ws::ws))
        .route("/graphql", post(graphql))
        // GitHub-compatible REST edge — every request is forwarded to the
        // in-process `GithubRouter`, so the real `gh` CLI and any GitHub client
        // work against this live server (was built but never mounted).
        .route("/user", any(github_forward))
        .route("/users/:login", any(github_forward))
        .route("/api/v1/version", any(github_forward))
        .route("/api/v3", any(github_forward))
        .route("/api/v3/user", any(github_forward))
        .route("/api/v3/users/:login", any(github_forward))
        .route("/api/v3/repos", any(repo_entry))
        .route("/api/v3/repos/*rest", any(repo_entry))
        .route("/api/v3/graphql", any(github_forward))
        .route("/repos", any(repo_entry))
        .route("/repos/*rest", any(repo_entry))
        // Explicitly catch gh auth login/device-flow attempts so agents get a
        // typed Jeryu repair path instead of falling through to the SPA.
        .route("/login/*rest", any(github_forward))
        .route("/api/v3/login/*rest", any(github_forward))
        // Steering: first-contact doc for a confused agent on the REST edge.
        .route("/.jeryu/agents/first-contact", any(github_forward))
        // Git smart-HTTP transport on the unified listener so `git clone`/`push`
        // work against this server. Mounted under `/git/` to stay clear of the
        // GitHub-shaped REST routes above: a root-level `:owner` param would
        // conflict with the literal `/repos`, `/users`, ... routes in the matcher.
        .merge(
            AxumRouter::new()
                .route(
                    "/git/:owner/:repo/info/refs",
                    get(crate::git_transport::git_info_refs),
                )
                .route(
                    "/git/:owner/:repo/git-upload-pack",
                    post(crate::git_transport::git_upload_pack),
                )
                .route(
                    "/git/:owner/:repo/git-receive-pack",
                    post(crate::git_transport::git_receive_pack),
                )
                .route(
                    "/git/:owner/:repo/info/lfs/objects/batch",
                    post(crate::git_transport::git_lfs_batch),
                )
                .route(
                    "/git/:owner/:repo/info/lfs/objects/:oid",
                    get(crate::git_transport::git_lfs_download)
                        .put(crate::git_transport::git_lfs_upload),
                )
                .route(
                    "/git/:owner/:repo/info/lfs/objects/:oid/verify",
                    post(crate::git_transport::git_lfs_verify),
                )
                .route(
                    "/git/:owner/:repo/info/lfs/locks/verify",
                    post(crate::git_transport::git_lfs_locks_verify),
                )
                .route_layer(DefaultBodyLimit::disable()),
        )
        .fallback(surface::spa_fallback)
        // Response middleware that stamps every reply with advisory steering
        // headers (and a per-route MCP tool hint for gh/automation UAs).
        .layer(from_fn(steer_headers))
        .layer(from_fn_with_state(state.clone(), auth::gate))
        .layer(from_fn(request_id::propagate))
        .with_state(state)
        .merge(mcp_router)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "jeryu-api" }))
}

const HDR_API: &str = "x-jeryu-api";
const HDR_FAST_PATH: &str = "x-jeryu-fast-path";
const HDR_TOOL: &str = "x-jeryu-tool";

/// Response middleware: stamps every reply with advisory steering headers. For
/// `gh`/automation user-agents it also injects a suggested jeryu MCP tool for
/// the request's route+method, nudging external agents off bespoke `gh`
/// invocations and onto the faster MCP path. Cheap and infallible: it never
/// fails the request and only ever appends headers.
async fn steer_headers(request: Request, next: Next) -> AxumResponse {
    let user_agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in advisory_headers(&user_agent, &method, &path) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            headers.insert(name, value);
        }
    }
    response
}

/// Pure builder for the advisory steering headers. Always emits the API version
/// and fast-path pointer; for `gh`/automation/agent user-agents it additionally
/// emits a per-route MCP tool hint when one is known. Factored out of the
/// middleware so the header policy can be unit-tested without a live server.
fn advisory_headers(
    user_agent: &str,
    method: &HttpMethod,
    path: &str,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        (HDR_API, "v4".to_string()),
        (HDR_FAST_PATH, "/.jeryu/capabilities".to_string()),
    ];
    if is_automation_agent(user_agent)
        && let Some(tool) = suggested_tool(method, path)
    {
        headers.push((HDR_TOOL, tool.to_string()));
    }
    headers
}

/// Heuristic: does this user-agent look like the `gh` CLI, a generic HTTP
/// client used by automation, or a Jeryu/agent UA? Matched case-insensitively.
fn is_automation_agent(user_agent: &str) -> bool {
    let ua = user_agent.to_ascii_lowercase();
    const NEEDLES: [&str; 7] = [
        "github cli",
        "go-gh",
        "okhttp",
        "curl",
        "python-requests",
        "jeryu",
        "agent",
    ];
    NEEDLES.iter().any(|needle| ua.contains(needle))
}

/// Suggests the jeryu MCP tool for a route+method so steered agents can switch
/// to the faster path. Mutations map to dedicated MCP tools; all other GETs map
/// to the generic read tool. Returns `None` when no hint applies.
fn suggested_tool(method: &HttpMethod, path: &str) -> Option<&'static str> {
    let trimmed = path.trim_end_matches('/');
    match *method {
        HttpMethod::POST if trimmed.ends_with("/pulls") => Some(MCP_PATCH_TOOL),
        HttpMethod::POST if trimmed.contains("/actions/") => Some(MCP_RUN_TESTS_TOOL),
        HttpMethod::PUT if trimmed.ends_with("/merge") => Some(MCP_MERGE_TOOL),
        HttpMethod::POST if trimmed.ends_with("/issues") => Some(MCP_ISSUE_TOOL),
        HttpMethod::GET if trimmed.contains("/actions/") => Some(MCP_CHECKS_TOOL),
        HttpMethod::GET if trimmed.contains("/check-runs") => Some(MCP_CHECKS_TOOL),
        HttpMethod::GET if trimmed.contains("/pulls") => Some(MCP_BLOCKERS_TOOL),
        HttpMethod::GET => Some(MCP_READ_TOOL),
        _ => None,
    }
}

/// Capability manifest: advertises the live endpoints plus a `gh` command -> jeryu
/// mapping so external agents can discover and prefer the faster MCP path.
async fn capabilities() -> Json<Value> {
    Json(capabilities_payload())
}

/// Pure builder for the `/.jeryu/capabilities` payload (unit-testable).
fn capabilities_payload() -> Value {
    json!({
        "server": "jeryu",
        "api_version": "v4",
        "graphql": "/graphql",
        "websocket": "/api/v1/ws",
        "mcp_endpoint": "/mcp",
        "mcp_tools": MCP_GUIDANCE_TOOLS,
        "gh_command_map": {
            "gh auth login": "Do not run direct gh auth against a Jeryu host; run jeryu gh-setup --host <local-jeryu-url> --token-file ~/.jeryu/secrets/merge-token instead.",
            "gh auth refresh": "Do not refresh host auth manually; rerun jeryu gh-setup --host <same-local-host> --token-file ~/.jeryu/secrets/merge-token for the Jeryu host entry.",
            "gh auth status": "If status fails for the Jeryu host, do not start a login flow; rerun jeryu gh-setup --host <same-local-host> --token-file ~/.jeryu/secrets/merge-token and inspect /.jeryu/capabilities.",
            "gh pr create": MCP_PATCH_TOOL,
            "gh pr merge": MCP_MERGE_TOOL,
            "gh pr list": "GET /repos/{owner}/{repo}/pulls",
            "gh workflow list": "GET /repos/{owner}/{repo}/actions/workflows",
            "gh workflow view": "GET /repos/{owner}/{repo}/actions/workflows/{workflow_id}",
            "gh run list": "GET /repos/{owner}/{repo}/actions/runs",
            "gh run view": "GET /repos/{owner}/{repo}/actions/runs/{id}",
            "gh workflow run": MCP_RUN_TESTS_TOOL,
            "gh run rerun": MCP_RUN_TESTS_TOOL,
            "gh run cancel": MCP_RUN_TESTS_TOOL,
            "gh issue create": MCP_ISSUE_TOOL,
            "gh api": "Use /.jeryu/capabilities and the listed jeryu.* MCP tools; unsupported REST returns guided JSON.",
            "gh repo create": "POST /repos",
        },
        "gh_auth_policy": {
            "do_not_run": ["gh auth login", "gh auth refresh", "credential-store token hunting"],
            "run_instead": GH_SETUP_COMMAND,
            "token_file": GH_SETUP_TOKEN_FILE,
            "stale_host_repair": "jeryu gh-setup --host <same-local-host> --token-file ~/.jeryu/secrets/merge-token",
            "host_auth_boundary": GH_AUTH_BOUNDARY,
            "agent_auth": "jeryu agent auth doctor <tool>; jeryu agent auth import --from-host <tool>",
        },
        "fast_path_advice":
            "Prefer the jeryu MCP tools for mutations; gh REST/GraphQL is supported but slower.",
    })
}

async fn bootstrap(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
) -> AxumResponse {
    match bootstrap_payload_for_user(&state, &account) {
        Ok(payload) => Json(payload).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "serialization_failed",
            &format!("bootstrap payload serialization failed: {err}"),
        ),
    }
}

async fn bootstrap_tui(State(state): State<Arc<WebState>>) -> Json<TuiReadModel> {
    Json(workcells::live_tui(&state))
}

/// `GET /api/v1/ecosystem` — the live ecosystem tool-graph for generic external
/// clients. Sources real data from the MCP catalog, the forge, and the live
/// read-model; read-only, never mutates state.
async fn ecosystem(State(state): State<Arc<WebState>>) -> AxumResponse {
    Json(ecosystem::ecosystem_response(state.github.core())).into_response()
}

/// `GET /api/v1/ci/runs/{id}/evidence` — the derived evidence list for a CI run
/// (a check-run keyed by UUID). Returns a structured 404 when the run id does
/// not resolve to a live run, never a silent empty list.
async fn ci_run_evidence(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    match ci_evidence::run_evidence(state.github.core(), &account, &id) {
        Some(evidence) => Json(evidence).into_response(),
        None => ci_evidence_not_found_error(),
    }
}

pub(super) fn server_time() -> String {
    chrono_like_now()
}

pub(super) fn chrono_like_now() -> String {
    jeryu_readmodel::TuiReadModel::default()
        .generated_at
        .to_rfc3339()
}

pub(super) fn api_error(status: StatusCode, code: &str, message: &str) -> AxumResponse {
    (status, Json(json!({ "code": code, "message": message }))).into_response()
}

fn ci_evidence_not_found_error() -> AxumResponse {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "code": "not_found",
            "message": "ci run not found",
            "purpose": "retrieve evidence for one live CI run",
            "reason": "the supplied run id is malformed or does not match any check-run in the live forge",
            "common_fixes": [
                "request a run id returned by GET /repos/{owner}/{repo}/actions/runs",
                "request a check-run id from GET /repos/{owner}/{repo}/commits/{sha}/check-runs",
                "retry after the push-to-CI bridge has registered check-runs for the commit"
            ],
            "docs_url": "/docs/api/ci-run-evidence",
            "repair_hint": "use a live check-run UUID, then retry GET /api/v1/ci/runs/{id}/evidence",
        })),
    )
        .into_response()
}
