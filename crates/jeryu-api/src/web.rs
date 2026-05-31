//! Axum HTTP/WebSocket edge for the local live Jeryu API.

mod markdown;
mod permissions;

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{OriginalUri, Path as AxumPath, Request, State};
use axum::http::{HeaderName, HeaderValue, Method as HttpMethod, StatusCode, header};
use axum::middleware::{Next, from_fn};
use axum::response::{Html, IntoResponse, Response as AxumResponse};
use axum::routing::{any, get, post};
use axum::{Json, Router as AxumRouter};
use futures_util::StreamExt;
use jeryu_core::{CheckConclusion, CheckRunStatus, ForgeCore, PullRequestState, Repository};
use jeryu_readmodel::contracts::{
    AvailableAction, BlobEncoding, BlobResponse, EntityHandle, RefKind, RefSelectorItem,
    RenderedMarkdown, RepositoryFacets, RepositoryId, RepositoryListResponse, RepositorySummary,
    RepositoryVisibility, ServerWsMessage, TreeEntry, Viewer, WebBootstrap, WebEvent,
    WebFeatureFlags,
};
use jeryu_readmodel::{
    Bottleneck, ComponentHealth, PoolActivity, PoolRollup, RepoActivity, RunnerHealth,
    SystemHealth, TuiReadModel,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};

use crate::{GithubRouter, Method, Response as GithubResponse};
use markdown::render_markdown;
use permissions::permissions;

const WS_PROTOCOL: &str = "jeryu.ws.v1";
const MCP_READ_TOOL: &str = "jeryu.get_system_snapshot";
const MCP_CHECKS_TOOL: &str = "jeryu.get_ci_run_jobs";
const MCP_BLOCKERS_TOOL: &str = "jeryu.explain_blockers";
const MCP_PATCH_TOOL: &str = "jeryu.propose_patch";
const MCP_MERGE_TOOL: &str = "jeryu.request_merge";
const MCP_ISSUE_TOOL: &str = "jeryu.bug_submit";
const MCP_GUIDANCE_TOOLS: &[&str] = &[
    MCP_READ_TOOL,
    MCP_CHECKS_TOOL,
    MCP_BLOCKERS_TOOL,
    MCP_PATCH_TOOL,
    MCP_MERGE_TOOL,
    MCP_ISSUE_TOOL,
];

#[derive(Clone, Debug)]
pub struct WebServerConfig {
    pub bind: SocketAddr,
    pub spa_dir: PathBuf,
    pub data_dir: PathBuf,
}

#[derive(Clone)]
struct WebState {
    github: GithubRouter,
    tui: TuiReadModel,
    /// Live-stream fan-out hub: hands out monotonic sequence numbers and keeps
    /// a subscriber registry so the WS edge can push snapshots/deltas per scope.
    ws: WsHub,
}

impl WebState {
    fn new(core: ForgeCore) -> Self {
        // Assemble a LIVE read model from ForgeCore state so the TUI/web panes
        // render real pool activity and system health, not the empty fixture.
        let tui = assemble_read_model(&core);
        Self {
            github: GithubRouter::with_core(core),
            tui,
            ws: WsHub::new(),
        }
    }
}

/// Build a populated [`TuiReadModel`] from live [`ForgeCore`] state.
///
/// For every repository on the server we roll up its open pull requests and
/// check-runs into a [`RepoActivity`], classifying each check-run by status:
/// `Queued` → queued, `InProgress` → running, and any `Completed` run whose
/// conclusion is `Failure` → failed. The per-repo counts are then aggregated
/// into a single synthetic `default` [`PoolRollup`] so the Pools/Health pane has
/// a real, non-empty fabric to render. [`SystemHealth`] reports every component
/// (`scm`/`database`/`sandbox`/`cache`/`vault`) as Healthy because holding a
/// live `ForgeCore` means the local plane is open and serving.
fn assemble_read_model(core: &ForgeCore) -> TuiReadModel {
    TuiReadModel {
        pool_activity: assemble_pool_activity(core),
        system: healthy_system(),
        ..TuiReadModel::default()
    }
}

/// Roll up every repo's PRs + check-runs into [`PoolActivity`].
fn assemble_pool_activity(core: &ForgeCore) -> PoolActivity {
    let mut repos: Vec<RepoActivity> = Vec::new();
    let mut default_pool = PoolRollup::new("default");

    for repo in core.list_repositories(None) {
        let checks = core
            .list_check_runs(&repo.owner, &repo.name, None)
            .map(|runs| runs.check_runs)
            .unwrap_or_default();

        let mut queued = 0u32;
        let mut running = 0u32;
        let mut failed = 0u32;
        for check in &checks {
            match check.status {
                CheckRunStatus::Queued => queued = queued.saturating_add(1),
                CheckRunStatus::InProgress => running = running.saturating_add(1),
                CheckRunStatus::Completed => {
                    if check.conclusion == Some(CheckConclusion::Failure) {
                        failed = failed.saturating_add(1);
                    }
                }
            }
        }

        // A repo with neither open PRs nor any check-run is not active work; skip
        // it so the activity rollup reflects real load rather than every repo.
        let open_pulls = core
            .list_pull_requests(&repo.owner, &repo.name, None)
            .map(|pulls| {
                pulls
                    .iter()
                    .filter(|pr| {
                        !matches!(
                            pr.state,
                            PullRequestState::Closed | PullRequestState::Merged
                        )
                    })
                    .count() as u32
            })
            .unwrap_or(0);
        if open_pulls == 0 && checks.is_empty() {
            continue;
        }

        default_pool.queued_jobs = default_pool.queued_jobs.saturating_add(queued);
        default_pool.running_jobs = default_pool.running_jobs.saturating_add(running);
        default_pool.failed_jobs = default_pool.failed_jobs.saturating_add(failed);

        repos.push(RepoActivity {
            repo: repo.full_name.clone(),
            queued_jobs: queued,
            running_jobs: running,
            failed_jobs: failed,
            pools: vec!["default".to_string()],
        });
    }

    // Size the synthetic pool's capacity to the running load so utilization is
    // meaningful and the pool only shows saturated when work is genuinely queued
    // with no idle slot. With no work at all, leave a single idle slot.
    default_pool.active_slots = default_pool.running_jobs.max(1);
    default_pool.configured_max_slots = default_pool.active_slots;
    default_pool.online_runners = default_pool.active_slots;

    // Only surface the pool once there is at least one active repo; an empty
    // server yields an empty (Unknown-health) activity rollup, never a fake pool.
    let pools = if repos.is_empty() {
        Vec::new()
    } else {
        vec![default_pool]
    };

    PoolActivity {
        repos,
        pools,
        ..PoolActivity::default()
    }
}

/// All system components reported Healthy: holding a live `ForgeCore` means the
/// local control plane (scm/db/sandbox/cache/vault) is open and serving.
fn healthy_system() -> SystemHealth {
    SystemHealth {
        scm: ComponentHealth::ok("scm", 0),
        database: ComponentHealth::ok("database", 0),
        sandbox: ComponentHealth::ok("sandbox", 0),
        cache: ComponentHealth::ok("cache", 0),
        vault: ComponentHealth::ok("vault", 0),
        runners: RunnerHealth::default(),
    }
}

/// Live-stream fan-out hub for the WebSocket event spine.
///
/// The tokio `sync` feature is intentionally NOT enabled in this crate, so this
/// is a deliberately minimal `Arc<Mutex<_>>` registry rather than a
/// `tokio::sync::broadcast`. It hands out the server-wide monotonic event
/// sequence and tracks which scopes each live connection is subscribed to, so a
/// future producer can fan deltas out to exactly the interested connections.
/// The snapshot-on-subscribe path below works entirely through this hub today.
#[derive(Clone, Default)]
struct WsHub {
    inner: Arc<Mutex<WsHubInner>>,
}

#[derive(Default)]
struct WsHubInner {
    /// Server-wide monotonic event sequence; never reused, never decreases.
    next_seq: u64,
    /// Live connections, in registration order. Each tracks its own scopes.
    connections: Vec<WsConnection>,
}

/// A single live WebSocket connection's subscription state inside the hub.
struct WsConnection {
    id: u64,
    scopes: BTreeSet<String>,
}

impl WsHub {
    fn new() -> Self {
        Self::default()
    }

    /// Allocate the next monotonic event sequence number.
    fn next_seq(&self) -> u64 {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        inner.next_seq = inner.next_seq.saturating_add(1);
        inner.next_seq
    }

    /// The highest sequence handed out so far (0 before any event).
    fn current_seq(&self) -> u64 {
        self.inner.lock().expect("ws hub mutex poisoned").next_seq
    }

    /// Register a fresh connection and return its hub-unique id.
    fn register(&self) -> u64 {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        let id = inner
            .next_seq
            .wrapping_add(inner.connections.len() as u64 + 1);
        inner.connections.push(WsConnection {
            id,
            scopes: BTreeSet::new(),
        });
        id
    }

    /// Replace the scope set a connection is subscribed to.
    fn set_scopes(&self, id: u64, scopes: &BTreeSet<String>) {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        if let Some(conn) = inner.connections.iter_mut().find(|c| c.id == id) {
            conn.scopes = scopes.clone();
        }
    }

    /// Drop scopes from a connection's subscription set.
    fn remove_scopes(&self, id: u64, scopes: &[String]) {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        if let Some(conn) = inner.connections.iter_mut().find(|c| c.id == id) {
            for scope in scopes {
                conn.scopes.remove(scope);
            }
        }
    }

    /// Forget a connection entirely (on socket close).
    fn unregister(&self, id: u64) {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        inner.connections.retain(|c| c.id != id);
    }
}

pub async fn serve(config: WebServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&config.data_dir)?;
    let db_path = config.data_dir.join("forge.sqlite");
    let core = ForgeCore::open_sqlite(db_path)?;
    let app = app(WebState::new(core), &config.spa_dir);
    let listener = TcpListener::bind(config.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn app(state: WebState, spa_dir: &Path) -> AxumRouter {
    let spa = ServeDir::new(spa_dir).fallback(ServeFile::new(spa_dir.join("index.html")));
    let mcp_state = Arc::new(jeryu_mcp::McpHttpState::new(Arc::new(
        jeryu_mcp::MemoryBackend::new(),
    )));
    AxumRouter::new()
        .route("/health", get(health))
        // Steering surface: advertises the faster jeryu/MCP path so external
        // agents stuck on bespoke `gh` commands can discover it.
        .route("/.jeryu/capabilities", get(capabilities))
        .route("/api/v1/bootstrap", get(bootstrap))
        .route("/api/v1/bootstrap.tui", get(bootstrap_tui))
        .route("/api/v1/repos", get(repos))
        .route("/api/v1/repos/:id", get(repo_detail))
        .route("/api/v1/repos/:id/refs", get(repo_refs))
        .route("/api/v1/repos/:id/tree", get(repo_tree))
        .route("/api/v1/repos/:id/blob", get(repo_blob))
        .route("/api/v1/repos/:id/raw", get(repo_raw))
        .route("/api/v1/repos/:id/readme", get(repo_readme))
        .route("/api/v1/markdown/render", post(markdown_render))
        .route("/api/v1/ws", get(ws))
        .route("/graphql", post(graphql))
        // GitHub-compatible REST edge — every request is forwarded to the
        // in-process `GithubRouter`, so the real `gh` CLI and any GitHub client
        // work against this live server (was built but never mounted).
        .route("/user", any(github_forward))
        .route("/users/:login", any(github_forward))
        .route("/api/v1/version", any(github_forward))
        .route("/repos", any(github_forward))
        .route("/repos/:owner/:repo", any(github_forward))
        .route("/repos/:owner/:repo/pulls", any(github_forward))
        .route("/repos/:owner/:repo/pulls/:number", any(github_forward))
        .route(
            "/repos/:owner/:repo/pulls/:number/merge",
            any(github_forward),
        )
        .route("/repos/:owner/:repo/issues", any(github_forward))
        .route(
            "/repos/:owner/:repo/issues/:number/comments",
            any(github_forward),
        )
        .route(
            "/repos/:owner/:repo/commits/:ref/status",
            any(github_forward),
        )
        .route(
            "/repos/:owner/:repo/commits/:ref/check-runs",
            any(github_forward),
        )
        .route("/repos/:owner/:repo/statuses/:sha", any(github_forward))
        .route("/repos/:owner/:repo/check-runs", any(github_forward))
        .route(
            "/repos/:owner/:repo/branches/:branch/protection",
            any(github_forward),
        )
        .route("/repos/:owner/:repo/releases", any(github_forward))
        .route("/repos/:owner/:repo/hooks", any(github_forward))
        // GitHub Actions edge (sourced from check-runs as a CI proxy) so
        // `gh run list` / `gh workflow list` work against this server.
        .route("/repos/:owner/:repo/actions/runs", any(github_forward))
        .route("/repos/:owner/:repo/actions/runs/:id", any(github_forward))
        .route(
            "/repos/:owner/:repo/actions/runs/:id/jobs",
            any(github_forward),
        )
        .route("/repos/:owner/:repo/actions/workflows", any(github_forward))
        // Steering: first-contact doc for a confused agent on the REST edge.
        .route("/.jeryu/agents/first-contact", any(github_forward))
        .route("/repos/:owner/:repo/*rest", any(github_forward))
        .fallback_service(spa)
        // Response middleware that stamps every reply with advisory steering
        // headers (and a per-route MCP tool hint for gh/automation UAs).
        .layer(from_fn(steer_headers))
        .with_state(Arc::new(state))
        .merge(jeryu_mcp::mcp_router(mcp_state))
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
        HttpMethod::PUT if trimmed.ends_with("/merge") => Some(MCP_MERGE_TOOL),
        HttpMethod::POST if trimmed.ends_with("/issues") => Some(MCP_ISSUE_TOOL),
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
            "gh pr create": MCP_PATCH_TOOL,
            "gh pr merge": MCP_MERGE_TOOL,
            "gh pr list": "GET /repos/{owner}/{repo}/pulls",
            "gh issue create": MCP_ISSUE_TOOL,
            "gh api": "Use /.jeryu/capabilities and the listed jeryu.* MCP tools; unsupported REST returns guided JSON.",
            "gh repo create": "POST /repos",
        },
        "fast_path_advice":
            "Prefer the jeryu MCP tools for mutations; gh REST/GraphQL is supported but slower.",
    })
}

async fn bootstrap(State(state): State<Arc<WebState>>) -> AxumResponse {
    match bootstrap_payload(&state) {
        Ok(payload) => Json(payload).into_response(),
        Err(err) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "serialization_failed",
            &format!("bootstrap payload serialization failed: {err}"),
        ),
    }
}

async fn bootstrap_tui(State(state): State<Arc<WebState>>) -> Json<TuiReadModel> {
    Json(state.tui.clone())
}

async fn repos(State(state): State<Arc<WebState>>) -> Json<RepositoryListResponse> {
    Json(repo_list_response(&state))
}

async fn repo_detail(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    match find_repo(&state, &id) {
        Some(repo) => Json(repo_summary(&state, &repo)).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "repository not found"),
    }
}

async fn repo_refs(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Json(vec![RefSelectorItem {
        name: repo.default_branch,
        sha: "unknown".to_string(),
        kind: RefKind::Branch,
        protected: state
            .github
            .core()
            .get_branch_protection(&repo.owner, &repo.name, "main")
            .is_ok(),
    }])
    .into_response()
}

async fn repo_tree(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    if find_repo(&state, &id).is_none() {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    }
    Json(Vec::<TreeEntry>::new()).into_response()
}

async fn repo_blob(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Json(BlobResponse {
        repo: repo_id(&repo),
        path: "README.md".to_string(),
        ref_name: repo.default_branch,
        sha: "unknown".to_string(),
        size_bytes: 0,
        mime: "text/markdown".to_string(),
        encoding: BlobEncoding::Utf8,
        text: Some(String::new()),
        base64: None,
        rendered_markdown: Some(render_markdown(&format!("# {}\n", repo.full_name))),
        is_binary: false,
    })
    .into_response()
}

async fn repo_raw(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Html(format!("# {}\n", repo.full_name)).into_response()
}

async fn repo_readme(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "repository not found");
    };
    Json(render_markdown(&format!(
        "# {}\n\nRepository metadata is live. Source import has not attached a README yet.\n",
        repo.full_name
    )))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct MarkdownRequest {
    #[serde(default)]
    markdown: String,
}

async fn markdown_render(Json(request): Json<MarkdownRequest>) -> Json<RenderedMarkdown> {
    Json(render_markdown(&request.markdown))
}

async fn graphql(State(state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    let body = std::str::from_utf8(&body).unwrap_or_default();
    github_response(state.github.handle(Method::Post, "/graphql", body))
}

/// Forwards a GitHub-compatible REST request to the in-process [`GithubRouter`],
/// which routes by `(method, path)` and renders GitHub-shaped JSON. The original
/// request path is forwarded verbatim so the dispatcher's segment matching works
/// unchanged; an unsupported HTTP verb returns a GitHub-shaped `405`.
async fn github_forward(
    State(state): State<Arc<WebState>>,
    method: HttpMethod,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> AxumResponse {
    let Some(method) = map_method(&method) else {
        return guided_github_edge_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed",
            "route unsupported GitHub-compatible REST method",
            "the Jeryu GitHub edge accepts GET, POST, and PUT for the guided compatibility subset",
            uri.path(),
        );
    };
    // Forward the path *and* query verbatim. The dispatcher splits the query
    // off for RFC5988 list pagination (`?per_page=&page=`); unrecognized query
    // keys are ignored rather than rejected, so `gh --paginate` works.
    let path_and_query = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_string(), ToString::to_string);
    let body = std::str::from_utf8(&body).unwrap_or_default();
    github_response(state.github.handle(method, &path_and_query, body))
}

fn guided_github_edge_response(
    status: StatusCode,
    message: &str,
    purpose: &str,
    reason: &str,
    path: &str,
) -> AxumResponse {
    (
        status,
        Json(json!({
            "message": message,
            "documentation_url": "/docs/rest",
            "jeryu_repair_hint": {
                "purpose": purpose,
                "reason": reason,
                "common_fixes": [
                    "retry with one of the listed GitHub-compatible REST routes",
                    "use /.jeryu/capabilities to choose a typed jeryu.* MCP tool",
                    "add a conformance test before widening the compatibility subset"
                ],
                "docs_url": "/docs/rest",
                "repair_hint": "prefer the listed Jeryu MCP/API alternatives, then rerun cargo test -p jeryu-api --features web"
            },
            "jeryu_mcp_tools": MCP_GUIDANCE_TOOLS,
            "jeryu_api_routes": [
                "GET /user",
                "GET /repos",
                "GET /repos/{owner}/{repo}",
                "GET /repos/{owner}/{repo}/pulls",
                "GET /repos/{owner}/{repo}/issues",
                "GET /repos/{owner}/{repo}/commits/{ref}/status",
                "GET /repos/{owner}/{repo}/commits/{ref}/check-runs",
                "POST /graphql"
            ],
            "path": path,
        })),
    )
        .into_response()
}

/// Maps the HTTP verbs the GitHub edge supports to the dispatcher's [`Method`].
fn map_method(method: &HttpMethod) -> Option<Method> {
    match *method {
        HttpMethod::GET => Some(Method::Get),
        HttpMethod::POST => Some(Method::Post),
        HttpMethod::PUT => Some(Method::Put),
        _ => None,
    }
}

async fn ws(ws: WebSocketUpgrade, State(state): State<Arc<WebState>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<WebState>) {
    let conn_id = state.ws.register();
    let _ = send_server_message(&mut socket, hello_message(&state)).await;
    // Per-connection scope subscription set, mirrored into the hub registry.
    let mut scopes: BTreeSet<String> = BTreeSet::new();
    while let Some(message) = socket.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    match value.get("type").and_then(Value::as_str) {
                        Some("ping") => {
                            let nonce = value
                                .get("nonce")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let _ = send_server_message(
                                &mut socket,
                                ServerWsMessage::Pong {
                                    nonce,
                                    server_time: server_time(),
                                },
                            )
                            .await;
                        }
                        Some("hello") => {
                            // A `hello` may carry an initial subscription set.
                            for scope in requested_scopes(&value) {
                                scopes.insert(scope);
                            }
                            state.ws.set_scopes(conn_id, &scopes);
                            let _ = send_server_message(&mut socket, hello_message(&state)).await;
                            send_scope_snapshots(&mut socket, &state, &scopes).await;
                        }
                        Some("subscribe") => {
                            // Track the newly requested scopes and immediately push
                            // a snapshot Event frame for each, so the client paints
                            // from live read-model data without waiting for a delta.
                            let added: Vec<String> = requested_scopes(&value);
                            for scope in &added {
                                scopes.insert(scope.clone());
                            }
                            state.ws.set_scopes(conn_id, &scopes);
                            let snapshot_scopes: BTreeSet<String> = added.into_iter().collect();
                            send_scope_snapshots(&mut socket, &state, &snapshot_scopes).await;
                        }
                        Some("unsubscribe") => {
                            let dropped = unsubscribe_scopes(&value);
                            for scope in &dropped {
                                scopes.remove(scope);
                            }
                            state.ws.remove_scopes(conn_id, &dropped);
                        }
                        Some("ack") => {}
                        _ => {
                            let _ = send_server_message(
                                &mut socket,
                                ServerWsMessage::Error {
                                    code: "unknown_message".to_string(),
                                    message: "unsupported websocket message type".to_string(),
                                },
                            )
                            .await;
                        }
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
    state.ws.unregister(conn_id);
}

/// Extract subscription scopes from a `hello`/`subscribe` frame. Both carry
/// `subscriptions: [{ scope, filters }]` per the [`ClientWsMessage`] contract.
fn requested_scopes(value: &Value) -> Vec<String> {
    value
        .get("subscriptions")
        .and_then(Value::as_array)
        .map(|specs| {
            specs
                .iter()
                .filter_map(|spec| spec.get("scope").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Extract the scope list from an `unsubscribe` frame (`scopes: [..]`).
fn unsubscribe_scopes(value: &Value) -> Vec<String> {
    value
        .get("scopes")
        .and_then(Value::as_array)
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Push one snapshot [`ServerWsMessage::Event`] frame per subscribed scope, each
/// stamped with a fresh monotonic sequence from the hub.
async fn send_scope_snapshots(socket: &mut WebSocket, state: &WebState, scopes: &BTreeSet<String>) {
    for scope in scopes {
        if let Some(event) = snapshot_event(state, scope) {
            let _ = send_server_message(socket, ServerWsMessage::Event { event }).await;
        }
    }
}

/// Build a snapshot [`WebEvent`] for a subscribed scope from the read model.
///
/// Supported scopes: `global.activity` (server-wide pool totals + bottlenecks),
/// `pool.{name}` (one pool's rollup), and `system.health` (component health).
/// Unknown scopes yield `None` and are silently ignored (no spurious frame).
fn snapshot_event(state: &WebState, scope: &str) -> Option<WebEvent> {
    let activity = &state.tui.pool_activity;
    let seq = state.ws.next_seq();
    let timestamp = server_time();
    if scope == "global.activity" {
        let totals = activity.totals();
        let bottlenecks: Vec<String> = activity
            .bottlenecks()
            .iter()
            .map(Bottleneck::describe)
            .collect();
        return Some(WebEvent {
            seq,
            timestamp,
            scope: scope.to_string(),
            kind: "activity.snapshot".to_string(),
            entity: "global".to_string(),
            summary: format!(
                "{} queued / {} running / {} failed across {} pool(s)",
                totals.queued_jobs, totals.running_jobs, totals.failed_jobs, totals.pools
            ),
            payload: json!({
                "health": activity.health(),
                "totals": totals,
                "bottlenecks": bottlenecks,
            }),
        });
    }
    if let Some(pool_name) = scope.strip_prefix("pool.") {
        let pool = activity.pools.iter().find(|p| p.pool == pool_name)?;
        let payload = serialize_payload(pool).ok()?;
        return Some(WebEvent {
            seq,
            timestamp,
            scope: scope.to_string(),
            kind: "pool.snapshot".to_string(),
            entity: pool.pool.clone(),
            summary: format!(
                "pool '{}': {} queued / {} running, {:.0}% utilized",
                pool.pool,
                pool.queued_jobs,
                pool.running_jobs,
                pool.utilization() * 100.0
            ),
            payload,
        });
    }
    if scope == "system.health" {
        let system = &state.tui.system;
        let payload = serialize_payload(system).ok()?;
        return Some(WebEvent {
            seq,
            timestamp,
            scope: scope.to_string(),
            kind: "system.snapshot".to_string(),
            entity: "system".to_string(),
            summary: "system component health snapshot".to_string(),
            payload,
        });
    }
    None
}

async fn send_server_message(socket: &mut WebSocket, message: ServerWsMessage) -> Result<(), ()> {
    let encoded = serde_json::to_string(&message).map_err(|_| ())?;
    socket.send(Message::Text(encoded)).await.map_err(|_| ())
}

fn hello_message(state: &WebState) -> ServerWsMessage {
    ServerWsMessage::Hello {
        server_time: server_time(),
        current_seq: state.ws.current_seq(),
        protocol: WS_PROTOCOL.to_string(),
    }
}

fn bootstrap_payload(state: &WebState) -> Result<WebBootstrap, serde_json::Error> {
    let repos = repo_summaries(state);
    let tui = serialize_payload(&state.tui)?;
    Ok(WebBootstrap {
        generated_at: state.tui.generated_at.to_rfc3339(),
        schema_version: "0.1.0-alpha".to_string(),
        viewer: Viewer {
            id: "local-operator".to_string(),
            login: "local".to_string(),
            display_name: Some("Local Operator".to_string()),
            avatar_url: None,
            global_permissions: permissions(),
        },
        tui,
        recent_repositories: repos.into_iter().take(10).collect(),
        websocket_url: "/api/v1/ws".to_string(),
        feature_flags: WebFeatureFlags {
            repo_create: false,
            settings_write: false,
            merge_write: false,
            markdown_html: true,
            agents: false,
            mcp: true,
        },
    })
}

fn serialize_payload<T: Serialize>(value: &T) -> Result<Value, serde_json::Error> {
    serde_json::to_value(value)
}

fn repo_list_response(state: &WebState) -> RepositoryListResponse {
    let repositories = repo_summaries(state);
    let mut owners = BTreeSet::new();
    for repo in &repositories {
        owners.insert(repo.id.owner.clone());
    }
    RepositoryListResponse {
        generated_at: state.tui.generated_at.to_rfc3339(),
        total: repositories.len() as u64,
        repositories,
        facets: RepositoryFacets {
            hosts: vec!["jeryu".to_string()],
            owners: owners.into_iter().collect(),
            families: Vec::new(),
            languages: Vec::new(),
        },
    }
}

fn repo_summaries(state: &WebState) -> Vec<RepositorySummary> {
    state
        .github
        .core()
        .list_repositories(None)
        .into_iter()
        .map(|repo| repo_summary(state, &repo))
        .collect()
}

fn repo_summary(state: &WebState, repo: &Repository) -> RepositorySummary {
    let pulls = state
        .github
        .core()
        .list_pull_requests(&repo.owner, &repo.name, None)
        .unwrap_or_default();
    let checks = state
        .github
        .core()
        .list_check_runs(&repo.owner, &repo.name, None)
        .map(|runs| runs.check_runs)
        .unwrap_or_default();
    RepositorySummary {
        id: repo_id(repo),
        entity: EntityHandle {
            kind: "repo".to_string(),
            id: repo.id.to_string(),
        },
        description: repo.description.clone(),
        visibility: if repo.private {
            RepositoryVisibility::Private
        } else {
            RepositoryVisibility::Public
        },
        default_branch: repo.default_branch.clone(),
        family: None,
        topics: Vec::new(),
        language: None,
        health: if checks
            .iter()
            .any(|check| check.conclusion == Some(CheckConclusion::Failure))
        {
            "warning".to_string()
        } else {
            "healthy".to_string()
        },
        open_pull_requests: pulls
            .iter()
            .filter(|pr| {
                !matches!(
                    pr.state,
                    PullRequestState::Closed | PullRequestState::Merged
                )
            })
            .count() as u32,
        failing_checks: checks
            .iter()
            .filter(|check| check.conclusion == Some(CheckConclusion::Failure))
            .count() as u32,
        running_jobs: checks
            .iter()
            .filter(|check| check.status == jeryu_core::CheckRunStatus::InProgress)
            .count() as u32,
        active_agents: 0,
        blocked_agents: 0,
        updated_at: repo.updated_at.to_rfc3339(),
        clone_http_url: Some(format!("/repos/{}.git", repo.full_name)),
        clone_ssh_url: None,
        available_actions: vec![AvailableAction {
            action_id: "repo.open".to_string(),
            label: "Open".to_string(),
            risk: None,
        }],
    }
}

fn repo_id(repo: &Repository) -> RepositoryId {
    RepositoryId {
        id: repo.id.to_string(),
        host: "jeryu".to_string(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
    }
}

fn find_repo(state: &WebState, id: &str) -> Option<Repository> {
    state
        .github
        .core()
        .list_repositories(None)
        .into_iter()
        .find(|repo| repo.id.to_string() == id || repo.full_name == id)
}

pub(super) fn server_time() -> String {
    chrono_like_now()
}

fn chrono_like_now() -> String {
    jeryu_readmodel::TuiReadModel::default()
        .generated_at
        .to_rfc3339()
}

fn api_error(status: StatusCode, code: &str, message: &str) -> AxumResponse {
    (status, Json(json!({ "code": code, "message": message }))).into_response()
}

fn github_response(response: GithubResponse) -> AxumResponse {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut axum_response = (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        response.body,
    )
        .into_response();
    // Surface the router's advisory headers on the wire: the overlap engine's
    // `X-Jeryu-Reused-PR` and the RFC5988 `Link` pagination header are carried
    // on `GithubResponse.headers`; without this passthrough they were dropped.
    let headers = axum_response.headers_mut();
    for (name, value) in response.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            headers.insert(name, value);
        }
    }
    axum_response
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_core::{CreateCheckRunRequest, CreatePullRequestRequest, CreateRepositoryRequest};
    use jeryu_readmodel::{HealthLevel, sample_read_model};

    /// Seed a repo + open PR + one failing check, build `WebState`, and assert
    /// the model served by `/api/v1/bootstrap.tui` (i.e. `state.tui`) reflects the
    /// seeded load: a populated `RepoActivity` with `failed_jobs == 1`, a non-empty
    /// pool fabric, and Healthy system components — NOT the empty fixture.
    #[tokio::test]
    async fn bootstrap_tui_reflects_seeded_repo_pr_and_failing_check() {
        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        // An open PR so the repo counts as active work.
        core.create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "feature".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("deadbeef".to_string()),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
        // A completed check-run that FAILED — must surface as one failed job.
        core.create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "ci".to_string(),
                head_sha: "deadbeef".to_string(),
                status: Some(CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Failure),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();

        let state = Arc::new(WebState::new(core));

        // The pool activity is genuinely populated, not the empty fixture.
        let activity = &state.tui.pool_activity;
        assert_eq!(activity.repos.len(), 1, "the seeded repo must be present");
        let repo = &activity.repos[0];
        assert_eq!(repo.repo, "alice/jeryu");
        assert_eq!(repo.failed_jobs, 1, "the failing check is one failed job");
        assert!(!activity.pools.is_empty(), "a default pool must roll up");
        assert_eq!(activity.pools[0].pool, "default");
        assert_eq!(activity.pools[0].failed_jobs, 1);

        // System health is Healthy (core is open), never the Unknown fixture.
        assert!(matches!(state.tui.system.scm.status, HealthLevel::Healthy));

        // The actual `/api/v1/bootstrap.tui` handler serves exactly this model.
        let served = bootstrap_tui(State(state.clone())).await.0;
        assert_eq!(served.pool_activity, *activity);
        assert_eq!(served.pool_activity.repos[0].failed_jobs, 1);
        // Sanity: this is NOT the empty default model.
        assert_ne!(
            served.pool_activity,
            TuiReadModel::default().pool_activity,
            "bootstrap.tui must not serve an empty pool activity"
        );
    }

    /// An empty server yields an empty pool fabric (Unknown health), and the
    /// fixture sample remains available purely as a test fallback.
    #[test]
    fn empty_server_assembles_empty_pool_activity_and_fixture_still_available() {
        let model = assemble_read_model(&ForgeCore::new());
        assert!(model.pool_activity.repos.is_empty());
        assert!(model.pool_activity.pools.is_empty());
        assert!(matches!(model.pool_activity.health(), HealthLevel::Unknown));
        // The fixture is still reachable as a fallback. Its `pool_activity` is the
        // empty default — exactly why serving it left the Pools pane blank, which
        // is what the live assembler above now replaces.
        assert!(sample_read_model().pool_activity.pools.is_empty());
    }

    #[test]
    fn bootstrap_and_repo_list_reflect_core_repositories() {
        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: true,
                description: Some("forge".to_string()),
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        let state = WebState::new(core);
        let bootstrap = bootstrap_payload(&state).expect("bootstrap serializes");
        assert_eq!(bootstrap.websocket_url, "/api/v1/ws");
        assert_eq!(bootstrap.recent_repositories.len(), 1);
        let repos = repo_list_response(&state);
        assert_eq!(repos.total, 1);
        assert_eq!(repos.repositories[0].id.owner, "alice");
    }

    #[test]
    fn markdown_renderer_escapes_html_and_builds_toc() {
        let rendered = render_markdown("# Hello <world>\n\nbody");
        assert!(rendered.html.contains("&lt;world&gt;"));
        assert_eq!(rendered.toc[0].id, "hello-world");
    }

    #[test]
    fn map_method_covers_supported_verbs_only() {
        assert!(matches!(map_method(&HttpMethod::GET), Some(Method::Get)));
        assert!(matches!(map_method(&HttpMethod::POST), Some(Method::Post)));
        assert!(matches!(map_method(&HttpMethod::PUT), Some(Method::Put)));
        assert!(map_method(&HttpMethod::DELETE).is_none());
        assert!(map_method(&HttpMethod::PATCH).is_none());
    }

    #[test]
    fn github_rest_edge_dispatches_repos_user_and_404() {
        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        let state = WebState::new(core);
        // The forwarder targets `state.github.handle(method, path, body)`; the
        // mounted `GET /repos` must return a GitHub-shaped 200 listing the repo.
        let repos = state.github.handle(Method::Get, "/repos", "");
        assert_eq!(repos.status, 200);
        assert!(repos.body.contains("alice"));
        assert!(repos.body.contains("jeryu"));
        // `GET /user` is mounted so `gh auth status` resolves a principal.
        assert_eq!(state.github.handle(Method::Get, "/user", "").status, 200);
        // An unknown route returns a clean GitHub-shaped 404, never a panic/500.
        assert_eq!(
            state
                .github
                .handle(Method::Get, "/repos/x/y/nope", "")
                .status,
            404
        );
    }

    #[test]
    fn app_router_builds_without_route_conflicts() {
        // Axum panics during construction on overlapping/ambiguous routes, so
        // building the full router is the regression guard for the REST mount,
        // the steering middleware layer, and the /.jeryu/capabilities route.
        let _app = app(
            WebState::new(ForgeCore::new()),
            std::path::Path::new("/tmp"),
        );
    }

    fn header_value<'a>(headers: &'a [(&'static str, String)], name: &str) -> Option<&'a str> {
        headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    fn known_mcp_tools() -> BTreeSet<String> {
        jeryu_mcp::tool_manifest()
            .into_iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect()
    }

    async fn response_json(response: AxumResponse) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body reads");
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|err| panic!("response body is not JSON ({err}): {bytes:?}"))
    }

    #[test]
    fn advisory_headers_always_present_on_any_route() {
        // A plain browser UA still gets the API + fast-path advisories, but no
        // tool hint (we only steer automation/gh-like clients).
        let headers = advisory_headers(
            "Mozilla/5.0 (browser)",
            &HttpMethod::GET,
            "/api/v1/bootstrap",
        );
        assert_eq!(header_value(&headers, HDR_API), Some("v4"));
        assert_eq!(
            header_value(&headers, HDR_FAST_PATH),
            Some("/.jeryu/capabilities")
        );
        assert!(header_value(&headers, HDR_TOOL).is_none());
    }

    #[test]
    fn advisory_headers_steer_gh_like_agents_to_mcp_tools() {
        // The gh CLI UA on a PR-create maps to the propose_patch MCP tool.
        let gh = advisory_headers(
            "GitHub CLI 2.40.0 go-gh/2.0",
            &HttpMethod::POST,
            "/repos/alice/jeryu/pulls",
        );
        assert_eq!(header_value(&gh, HDR_TOOL), Some(MCP_PATCH_TOOL));

        // A merge PUT maps to request_merge for any automation UA (curl here).
        let merge = advisory_headers(
            "curl/8.0",
            &HttpMethod::PUT,
            "/repos/alice/jeryu/pulls/7/merge",
        );
        assert_eq!(header_value(&merge, HDR_TOOL), Some(MCP_MERGE_TOOL));

        // GET PR routes steer to blocker explanation for agent UAs.
        let read = advisory_headers(
            "jeryu-agent/1.0",
            &HttpMethod::GET,
            "/repos/alice/jeryu/pulls",
        );
        assert_eq!(header_value(&read, HDR_TOOL), Some(MCP_BLOCKERS_TOOL));

        // Issue create gets a dedicated mutation tool.
        assert_eq!(
            header_value(
                &advisory_headers(
                    "python-requests/2.31",
                    &HttpMethod::POST,
                    "/repos/a/b/issues"
                ),
                HDR_TOOL
            ),
            Some(MCP_ISSUE_TOOL)
        );
    }

    #[test]
    fn automation_agent_detection_is_case_insensitive_and_scoped() {
        assert!(is_automation_agent("GitHub CLI 2.40.0"));
        assert!(is_automation_agent("github cli"));
        assert!(is_automation_agent("go-gh/2.0"));
        assert!(is_automation_agent("okhttp/4.12.0"));
        assert!(is_automation_agent("curl/8.4.0"));
        assert!(is_automation_agent("python-requests/2.31.0"));
        assert!(is_automation_agent("Jeryu-Agent/1.0"));
        assert!(is_automation_agent("some-agent-runner"));
        // A normal browser is not steered with a tool hint.
        assert!(!is_automation_agent(
            "Mozilla/5.0 (Macintosh) AppleWebKit Safari"
        ));
        assert!(!is_automation_agent(""));
    }

    #[test]
    fn suggested_tool_covers_mutations_and_reads() {
        assert_eq!(
            suggested_tool(&HttpMethod::POST, "/repos/a/b/pulls"),
            Some(MCP_PATCH_TOOL)
        );
        assert_eq!(
            suggested_tool(&HttpMethod::PUT, "/repos/a/b/pulls/3/merge"),
            Some(MCP_MERGE_TOOL)
        );
        assert_eq!(
            suggested_tool(&HttpMethod::GET, "/repos/a/b"),
            Some(MCP_READ_TOOL)
        );
        assert_eq!(
            suggested_tool(&HttpMethod::GET, "/repos/a/b/commits/deadbeef/check-runs"),
            Some(MCP_CHECKS_TOOL)
        );
        // A DELETE (unsupported verb) yields no hint.
        assert!(suggested_tool(&HttpMethod::DELETE, "/repos/a/b").is_none());
    }

    #[test]
    fn advertised_mcp_tools_exist_in_catalog() {
        let known = known_mcp_tools();
        for tool in MCP_GUIDANCE_TOOLS {
            assert!(known.contains(*tool), "missing MCP catalog tool: {tool}");
        }
        for tool in [
            suggested_tool(&HttpMethod::POST, "/repos/a/b/pulls"),
            suggested_tool(&HttpMethod::PUT, "/repos/a/b/pulls/3/merge"),
            suggested_tool(&HttpMethod::GET, "/repos/a/b/commits/deadbeef/check-runs"),
            suggested_tool(&HttpMethod::GET, "/repos/a/b/pulls"),
            suggested_tool(&HttpMethod::GET, "/repos/a/b"),
        ] {
            let tool = tool.expect("tool hint");
            assert!(known.contains(tool), "invalid suggested MCP tool: {tool}");
        }
        let payload = capabilities_payload();
        for tool in payload["mcp_tools"].as_array().expect("mcp_tools array") {
            let tool = tool.as_str().expect("tool string");
            assert!(known.contains(tool), "invalid capability MCP tool: {tool}");
        }
    }

    #[tokio::test]
    async fn live_unknown_github_route_returns_guided_json_not_spa() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let app = app(
            WebState::new(ForgeCore::new()),
            std::path::Path::new("/tmp/jeryu-no-spa"),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/repos/alice/jeryu/unknown-thing")
                    .header(header::USER_AGENT, "GitHub CLI 2.40.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let parsed = response_json(response).await;
        assert_eq!(
            parsed["jeryu_repair_hint"]["purpose"],
            "route unsupported GitHub-compatible REST request"
        );
        assert!(parsed["jeryu_mcp_tools"].as_array().unwrap().len() >= 4);
    }

    #[tokio::test]
    async fn live_unsupported_verb_returns_guided_json() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let app = app(
            WebState::new(ForgeCore::new()),
            std::path::Path::new("/tmp/jeryu-no-spa"),
        );
        let patch = app
            .oneshot(
                Request::builder()
                    .method(HttpMethod::PATCH)
                    .uri("/repos/alice/jeryu")
                    .header(header::USER_AGENT, "curl/8.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(patch.status(), StatusCode::METHOD_NOT_ALLOWED);
        let parsed = response_json(patch).await;
        assert_eq!(
            parsed["jeryu_repair_hint"]["purpose"],
            "route unsupported GitHub-compatible REST method"
        );
    }

    /// A list request with `?per_page`/`?page` now passes through (no longer a
    /// guided 501) and the RFC5988 `Link` header is surfaced on the wire via
    /// `github_response`'s header passthrough.
    #[tokio::test]
    async fn live_list_query_paginates_and_surfaces_link_header() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        // Two open PRs so a per_page=1 page leaves a `next`/`last` link.
        for (head, sha) in [("feat-a", "sha-a"), ("feat-b", "sha-b")] {
            core.create_pull_request(
                "alice",
                "jeryu",
                "alice",
                CreatePullRequestRequest {
                    title: head.to_string(),
                    head: head.to_string(),
                    base: "main".to_string(),
                    head_sha: Some(sha.to_string()),
                    ..CreatePullRequestRequest::default()
                },
            )
            .unwrap();
        }

        let response = app(
            WebState::new(core),
            std::path::Path::new("/tmp/jeryu-no-spa"),
        )
        .oneshot(
            Request::builder()
                .uri("/repos/alice/jeryu/pulls?per_page=1&page=1")
                .header(header::USER_AGENT, "go-gh/2.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let link = response
            .headers()
            .get("Link")
            .expect("Link header present")
            .to_str()
            .unwrap()
            .to_string();
        assert!(link.contains("rel=\"next\""), "Link has next: {link}");
        assert!(link.contains("rel=\"last\""), "Link has last: {link}");
        let parsed = response_json(response).await;
        assert_eq!(
            parsed.as_array().expect("pulls array").len(),
            1,
            "per_page=1 returns a single PR"
        );
    }

    /// The overlap engine's `X-Jeryu-Reused-PR` header reaches the wire through
    /// `github_response`'s passthrough when a create-PR request coalesces.
    #[tokio::test]
    async fn live_overlap_routing_surfaces_reused_pr_header() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        // An existing mergeable PR touching one file.
        core.create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "existing".to_string(),
                head: "feat-a".to_string(),
                base: "main".to_string(),
                head_sha: Some("sha-a".to_string()),
                changed_files: vec!["src/a.rs".to_string()],
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();

        let response = app(WebState::new(core), std::path::Path::new("/tmp/jeryu-no-spa"))
            .oneshot(
                Request::builder()
                    .method(HttpMethod::POST)
                    .uri("/repos/alice/jeryu/pulls")
                    .header(header::USER_AGENT, "go-gh/2.0")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"title":"hot-fix","head":"feat-a2","base":"main","changed_files":["src/a.rs"]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("X-Jeryu-Reused-PR")
                .expect("reused-pr header present")
                .to_str()
                .unwrap(),
            "1",
            "the header points at the reused PR number"
        );
    }

    #[tokio::test]
    async fn advertised_mcp_endpoint_is_mounted() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let response = app(
            WebState::new(ForgeCore::new()),
            std::path::Path::new("/tmp/jeryu-no-spa"),
        )
        .oneshot(Request::builder().uri("/mcp").body(Body::empty()).unwrap())
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[test]
    fn capabilities_payload_exposes_the_gh_command_map() {
        let payload = capabilities_payload();
        assert_eq!(payload["server"], "jeryu");
        assert_eq!(payload["api_version"], "v4");
        assert_eq!(payload["graphql"], "/graphql");
        assert_eq!(payload["websocket"], "/api/v1/ws");
        assert_eq!(payload["mcp_endpoint"], "/mcp");
        assert!(payload["fast_path_advice"].is_string());

        let map = &payload["gh_command_map"];
        for key in [
            "gh pr create",
            "gh pr merge",
            "gh pr list",
            "gh issue create",
            "gh api",
            "gh repo create",
        ] {
            assert!(map.get(key).is_some(), "missing gh_command_map key: {key}");
        }
        assert_eq!(map["gh pr create"], MCP_PATCH_TOOL);
        assert_eq!(map["gh pr merge"], MCP_MERGE_TOOL);
        assert_eq!(map["gh issue create"], MCP_ISSUE_TOOL);
        assert_eq!(map["gh repo create"], "POST /repos");
    }

    #[test]
    fn payload_serialization_errors_are_not_silently_replaced() {
        struct FailingSerialize;

        impl serde::Serialize for FailingSerialize {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(<S::Error as serde::ser::Error>::custom("synthetic failure"))
            }
        }

        assert!(serialize_payload(&FailingSerialize).is_err());
    }

    /// A `WebState` whose read model has one saturated pool, so the activity
    /// and pool scopes produce non-trivial snapshot frames.
    fn ws_state_with_pool() -> WebState {
        use jeryu_readmodel::{PoolActivity, PoolRollup, RepoActivity};
        let mut state = WebState::new(ForgeCore::new());
        let mut pool = PoolRollup::new("trusted");
        pool.active_slots = 2;
        pool.running_jobs = 2;
        pool.queued_jobs = 3; // saturated
        pool.online_runners = 2;
        state.tui.pool_activity = PoolActivity {
            repos: vec![RepoActivity {
                repo: "alice/jeryu".into(),
                queued_jobs: 3,
                running_jobs: 2,
                ..RepoActivity::default()
            }],
            pools: vec![pool],
            ..PoolActivity::default()
        };
        state
    }

    #[test]
    fn subscribe_frame_yields_scopes_and_snapshot_events() {
        let state = ws_state_with_pool();
        // A real client `subscribe` frame per the ClientWsMessage contract.
        let frame = json!({
            "type": "subscribe",
            "subscriptions": [
                { "scope": "global.activity", "filters": {} },
                { "scope": "pool.trusted", "filters": {} },
                { "scope": "system.health", "filters": {} },
            ],
        });
        // It deserializes into the typed wire contract (format is genuine).
        let parsed: jeryu_readmodel::contracts::ClientWsMessage =
            serde_json::from_value(frame.clone()).expect("subscribe frame parses");
        assert!(matches!(
            parsed,
            jeryu_readmodel::contracts::ClientWsMessage::Subscribe { .. }
        ));

        // The handler's scope extractor pulls every requested scope.
        let scopes = requested_scopes(&frame);
        assert_eq!(scopes.len(), 3);

        // Each subscribed scope yields a monotonic Event snapshot frame.
        let mut last_seq = 0u64;
        for scope in &scopes {
            let event = snapshot_event(&state, scope)
                .unwrap_or_else(|| panic!("scope {scope} should produce a snapshot"));
            assert_eq!(&event.scope, scope);
            assert!(event.seq > last_seq, "seq must be strictly monotonic");
            last_seq = event.seq;
            // The frame round-trips as a ServerWsMessage::Event on the wire.
            let msg = ServerWsMessage::Event { event };
            let encoded = serde_json::to_string(&msg).unwrap();
            assert!(encoded.contains("\"type\":\"event\""));
            assert!(encoded.contains(scope.as_str()));
        }

        // The activity snapshot reports the saturated pool's bottleneck.
        let activity = snapshot_event(&state, "global.activity").unwrap();
        let bottlenecks = activity.payload.get("bottlenecks").unwrap();
        assert!(
            bottlenecks.as_array().is_some_and(|b| !b.is_empty()),
            "saturated pool must surface a bottleneck"
        );
    }

    #[test]
    fn unknown_scope_produces_no_snapshot() {
        let state = ws_state_with_pool();
        assert!(snapshot_event(&state, "pool.does-not-exist").is_none());
        assert!(snapshot_event(&state, "totally.unknown").is_none());
    }

    #[test]
    fn ws_hub_seq_is_monotonic_and_tracks_subscribers() {
        let hub = WsHub::new();
        assert_eq!(hub.current_seq(), 0);
        let a = hub.next_seq();
        let b = hub.next_seq();
        assert!(b > a);
        assert_eq!(hub.current_seq(), b);

        let conn = hub.register();
        let mut scopes = BTreeSet::new();
        scopes.insert("global.activity".to_string());
        scopes.insert("pool.trusted".to_string());
        hub.set_scopes(conn, &scopes);
        hub.remove_scopes(conn, &["pool.trusted".to_string()]);
        // Unregister must not panic and leaves the hub usable.
        hub.unregister(conn);
        assert!(hub.next_seq() > b);
    }

    #[test]
    fn hello_frame_reports_current_seq() {
        let state = ws_state_with_pool();
        // Hand out two sequences, then the hello frame must echo current_seq.
        let _ = state.ws.next_seq();
        let _ = state.ws.next_seq();
        match hello_message(&state) {
            ServerWsMessage::Hello { current_seq, .. } => assert_eq!(current_seq, 2),
            other => panic!("expected hello, got {other:?}"),
        }
    }

    #[test]
    fn unsubscribe_frame_extracts_scopes() {
        let frame = json!({ "type": "unsubscribe", "scopes": ["pool.trusted", "system.health"] });
        let dropped = unsubscribe_scopes(&frame);
        assert_eq!(dropped, vec!["pool.trusted", "system.health"]);
    }
}
