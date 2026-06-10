//! Axum HTTP/WebSocket edge for the local live Jeryu API.

mod agent_runs;
mod ci_evidence;
mod codegraph;
mod control_plane;
mod ecosystem;
mod markdown;
mod mcp_backend;
mod permissions;
mod pulls;
mod repositories;
mod sessions;
mod surface;
mod tool_build;
mod workcells;
mod workcells_support;
mod ws;

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Path as AxumPath, Request, State};
use axum::http::{HeaderName, HeaderValue, Method as HttpMethod, StatusCode, header};
use axum::middleware::{Next, from_fn};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::{any, get, post};
use axum::{Json, Router as AxumRouter};
use jeryu_codegraph::CodeGraphStore;
use jeryu_core::ForgeCore;
use jeryu_readmodel::TuiReadModel;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};

use crate::GithubRouter;
use crate::git_materializer::GitMaterializer;
use crate::github::{MCP_GUIDANCE_TOOLS, MCP_RUN_TESTS_TOOL};
use jeryu_gitd::{GitdConfig, RepoManager};
use jeryu_runner_oci::{CliContainerRuntime, ContainerLifecycle};
use jeryu_runnerd::{WarmPool, WorkcellManager};
use repositories::{
    repo_blob, repo_detail, repo_raw, repo_readme, repo_readme_update, repo_refs, repo_tree, repos,
};
use surface::{bootstrap_payload, github_forward, graphql, markdown_render, repo_entry};

const WS_PROTOCOL: &str = "jeryu.ws.v1";
const MCP_READ_TOOL: &str = "jeryu.get_system_snapshot";
const MCP_CHECKS_TOOL: &str = "jeryu.get_ci_run_jobs";
const MCP_BLOCKERS_TOOL: &str = "jeryu.explain_blockers";
const MCP_PATCH_TOOL: &str = "jeryu.propose_patch";
const MCP_MERGE_TOOL: &str = "jeryu.request_merge";
const MCP_ISSUE_TOOL: &str = "jeryu.bug_submit";
/// Steady-state depth of pre-warmed agent containers the pool refills back to, so
/// a New Session claims a ready cell instead of paying a cold-start.
const WARM_POOL_TARGET: usize = 2;

#[derive(Clone, Debug)]
pub struct WebServerConfig {
    pub bind: SocketAddr,
    pub spa_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Storage root for bare git repositories served over smart-HTTP.
    pub git_storage_root: PathBuf,
}

#[derive(Clone)]
pub(crate) struct WebState {
    github: GithubRouter,
    tui: TuiReadModel,
    pub(crate) spa_dir: PathBuf,
    /// Live-stream fan-out hub: hands out monotonic sequence numbers and keeps
    /// a subscriber registry so the WS edge can push snapshots/deltas per scope.
    ws: WsHub,
    /// In-memory workcell controller for claim/repair/export/release flows.
    pub(crate) workcells: Arc<Mutex<WorkcellManager>>,
    /// Live high-level agent-run registry and control channels.
    pub(crate) agent_runs: agent_runs::AgentRunStore,
    /// Auxiliary codegraph SQLite store for read-only oracle queries.
    pub(crate) codegraph_store: CodeGraphStore,
    /// Shared git-daemon repository manager backing the smart-HTTP transport.
    pub(crate) repo_manager: Arc<RepoManager>,
    /// Forge handle for the push->CI bridge (shares state with `github`).
    pub(crate) core: ForgeCore,
    /// Pool of pre-warmed agent containers a New Session claims from, so the
    /// launch reuses a ready cell with no cold-start. It needs `&mut self` to
    /// claim and refill, so it lives behind the same `Mutex` style the rest of
    /// `WebState` uses. Production wires the real CLI lifecycle (plan-only unless
    /// `JERYU_RUN_OCI=1`); tests inject a recording fake lifecycle so the claim
    /// path is exercised without Docker/Podman.
    pub(crate) warm_pool: Arc<Mutex<WarmPool>>,
    /// Which PTY backend a New Session agent runs under (native kernel sandbox vs.
    /// docker-backed live container) and the docker seam. Resolved once from
    /// `JERYU_AGENT_RUNTIME` / `JERYU_DOCKER_BIN`; a test injects it directly so it
    /// never mutates process-global env.
    pub(crate) session_runtime: sessions::SessionRuntimeConfig,
}

impl WebState {
    fn with_repo_manager(
        core: ForgeCore,
        repo_manager: Arc<RepoManager>,
        spa_dir: PathBuf,
    ) -> Self {
        // Assemble a LIVE read model from ForgeCore state so the TUI/web panes
        // render real pool activity and system health, not the empty fixture.
        let tui = crate::read_model::assemble_read_model(&core);
        // ForgeCore is Arc-backed, so this handle shares state with `github`.
        let core_handle = core.clone();
        #[cfg(test)]
        let codegraph_store = CodeGraphStore::open(std::env::temp_dir().join(format!(
            "jeryu-web-codegraph-{}.sqlite",
            jeryu_runner_core::receipt::now_ms()
        )))
        .expect("test codegraph store");
        #[cfg(not(test))]
        let codegraph_store = CodeGraphStore::open_default().expect("open codegraph store");
        // Pre-warm the agent pool over the real CLI lifecycle. With the OCI gate
        // closed this only records planned cells (no daemon), so construction is
        // infallible in every environment the web edge boots in.
        let warm_runtime: Arc<dyn ContainerLifecycle> = Arc::new(CliContainerRuntime);
        let warm_pool = Arc::new(Mutex::new(
            WarmPool::new(warm_runtime, WARM_POOL_TARGET).expect("pre-warm the agent pool"),
        ));
        Self {
            github: GithubRouter::with_core(core).with_repo_manager(repo_manager.clone()),
            tui,
            spa_dir,
            ws: WsHub::new(),
            workcells: Arc::new(Mutex::new(WorkcellManager::new())),
            agent_runs: agent_runs::AgentRunStore::new(),
            codegraph_store,
            repo_manager,
            core: core_handle,
            warm_pool,
            session_runtime: sessions::SessionRuntimeConfig::from_env(),
        }
    }

    /// Test-only constructor with a throwaway git storage root; the in-process
    /// router tests never exercise the smart-HTTP transport.
    #[cfg(test)]
    fn new(core: ForgeCore) -> Self {
        Self::with_repo_manager(
            core,
            Arc::new(RepoManager::new(GitdConfig::new(
                std::env::temp_dir().join("jeryu-web-test-git"),
            ))),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/dist"),
        )
    }

    /// Test-only constructor that roots the git `RepoManager` at `storage_root`
    /// so the workcell export slice gate can run a real `git diff` against a
    /// fixture bare repository.
    #[cfg(test)]
    fn new_with_git_storage(core: ForgeCore, storage_root: PathBuf) -> Self {
        Self::with_repo_manager(
            core,
            Arc::new(RepoManager::new(GitdConfig::new(storage_root))),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/dist"),
        )
    }

    /// Test-only constructor that roots the git `RepoManager` at `storage_root`
    /// AND injects a [`WarmPool`] built over the given container lifecycle, so the
    /// claim path can be driven with a recording `FakeContainerRuntime` (no
    /// Docker/Podman) while still resolving a real bare repository for branch
    /// registration. The pool pre-warms `warm_target` cells.
    #[cfg(test)]
    fn new_with_git_storage_and_warm_pool(
        core: ForgeCore,
        storage_root: PathBuf,
        warm_runtime: Arc<dyn ContainerLifecycle>,
        warm_target: usize,
    ) -> Self {
        let mut state = Self::new_with_git_storage(core, storage_root);
        state.warm_pool = Arc::new(Mutex::new(
            WarmPool::new(warm_runtime, warm_target).expect("pre-warm the test agent pool"),
        ));
        state
    }

    /// Test-only: override the session runtime backend + docker seam directly so a
    /// hermetic test drives the docker / native paths without mutating process-wide
    /// env (the crate forbids `unsafe`, so `std::env::set_var` is unavailable).
    #[cfg(test)]
    pub(crate) fn with_session_runtime(mut self, runtime: sessions::SessionRuntimeConfig) -> Self {
        self.session_runtime = runtime;
        self
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
    std::fs::create_dir_all(&config.git_storage_root)?;
    let db_path = config.data_dir.join("forge.sqlite");
    // Share one RepoManager between the create-repo materializer (so a created
    // repo gets a bare repo on disk) and the smart-HTTP transport (so it can be
    // cloned/pushed) — both rooted at the same git storage root.
    let repo_manager = Arc::new(RepoManager::new(GitdConfig::new(
        config.git_storage_root.clone(),
    )));
    let core = ForgeCore::open_sqlite(db_path)?
        .with_repo_materializer(Arc::new(GitMaterializer::new(repo_manager.clone())));
    let app = app(
        WebState::with_repo_manager(core, repo_manager, config.spa_dir.clone()),
        &config.spa_dir,
    );
    let listener = TcpListener::bind(config.bind).await?;
    // ConnectInfo gives the git handlers the peer address so the gitd auth layer
    // can apply its loopback-permissive policy.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn app(state: WebState, spa_dir: &Path) -> AxumRouter {
    let mut state = state;
    state.spa_dir = spa_dir.to_path_buf();
    let spa = ServeDir::new(spa_dir).fallback(ServeFile::new(spa_dir.join("index.html")));
    let state = Arc::new(state);
    let mcp_state = Arc::new(jeryu_mcp::McpHttpState::new(Arc::new(
        mcp_backend::WebMcpBackend::new(state.clone()),
    )));
    AxumRouter::new()
        .route("/health", get(health))
        // Steering surface: advertises the faster jeryu/MCP path so external
        // agents stuck on bespoke `gh` commands can discover it.
        .route("/.jeryu/capabilities", get(capabilities))
        .route("/api/v1/bootstrap", get(bootstrap))
        .route("/api/v1/bootstrap.tui", get(bootstrap_tui))
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
        .route("/api/v1/repos", get(repos))
        .route("/api/v1/repos/:id", get(repo_detail))
        // Repo-scoped agent sessions: launch a hardened session, and the live
        // per-repo agent-runs list the web Active-Agents page consumes.
        .route("/api/v1/repos/:id/sessions", post(sessions::create))
        .route("/api/v1/repos/:id/agent-runs", get(sessions::list))
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
            post(pulls::review),
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
                .route_layer(DefaultBodyLimit::disable()),
        )
        .fallback_service(spa)
        // Response middleware that stamps every reply with advisory steering
        // headers (and a per-route MCP tool hint for gh/automation UAs).
        .layer(from_fn(steer_headers))
        .with_state(state)
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
            "gh auth login": "Do not run direct gh auth against a Jeryu host; run jeryu gh-setup --host <local-jeryu-url> instead.",
            "gh auth refresh": "Do not refresh host auth manually; rerun jeryu gh-setup for the Jeryu host entry.",
            "gh auth status": "If status fails for the Jeryu host, do not start a login flow; rerun jeryu gh-setup and inspect /.jeryu/capabilities.",
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
            "run_instead": "jeryu gh-setup --host http://127.0.0.1:8787 --token JERYU-TOKEN",
            "agent_auth": "jeryu agent auth doctor <tool>; jeryu agent auth import --from-host <tool>",
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
    AxumPath(id): AxumPath<String>,
) -> AxumResponse {
    match ci_evidence::run_evidence(state.github.core(), &id) {
        Some(evidence) => Json(evidence).into_response(),
        None => ci_evidence_not_found_error(),
    }
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod agent_runs_tests;

#[cfg(test)]
mod workcell_surface_tests;
