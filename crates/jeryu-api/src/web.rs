//! Axum HTTP/WebSocket edge for the local live Jeryu API.

mod agent_runs;
mod ci_evidence;
mod ecosystem;
mod markdown;
mod permissions;
mod repositories;
mod steering;
mod surface;
mod workcells;
mod workcells_support;
mod ws;
mod ws_hub;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::middleware::from_fn;
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::{any, get, post};
use axum::{Json, Router as AxumRouter};
use jeryu_core::ForgeCore;
use jeryu_readmodel::TuiReadModel;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};

use crate::GithubRouter;
use crate::git_materializer::GitMaterializer;
use jeryu_gitd::{GitdConfig, RepoManager};
use jeryu_runnerd::WorkcellManager;
use repositories::{
    repo_blob, repo_detail, repo_raw, repo_readme, repo_readme_update, repo_refs, repo_tree, repos,
};
use steering::{capabilities, steer_headers};
use surface::{bootstrap_payload, github_forward, graphql, markdown_render, repo_entry};
use ws_hub::WsHub;

const WS_PROTOCOL: &str = "jeryu.ws.v1";

#[cfg(test)]
use crate::github::MCP_GUIDANCE_TOOLS;
#[cfg(test)]
use axum::extract::Request;
#[cfg(test)]
use axum::http::{Method as HttpMethod, header};
#[cfg(test)]
use steering::{
    HDR_API, HDR_FAST_PATH, HDR_TOOL, MCP_BLOCKERS_TOOL, MCP_CHECKS_TOOL, MCP_ISSUE_TOOL,
    MCP_MERGE_TOOL, MCP_PATCH_TOOL, MCP_READ_TOOL, advisory_headers, capabilities_payload,
    is_automation_agent, suggested_tool,
};

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
    /// Shared git-daemon repository manager backing the smart-HTTP transport.
    pub(crate) repo_manager: Arc<RepoManager>,
    /// Forge handle for the push->CI bridge (shares state with `github`).
    pub(crate) core: ForgeCore,
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
        Self {
            github: GithubRouter::with_core(core),
            tui,
            spa_dir,
            ws: WsHub::new(),
            workcells: Arc::new(Mutex::new(WorkcellManager::new())),
            repo_manager,
            core: core_handle,
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
        .route("/api/v1/agent-runs", post(agent_runs::start))
        .route("/api/v1/agent-runs/:id", get(agent_runs::status))
        .route("/api/v1/agent-runs/:id/control", post(agent_runs::control))
        .route(
            "/api/v1/agent-runs/:id/export_pr",
            post(agent_runs::export_pr),
        )
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
        .route("/api/v1/repos/:id/refs", get(repo_refs))
        .route("/api/v1/repos/:id/tree", get(repo_tree))
        .route("/api/v1/repos/:id/blob", get(repo_blob))
        .route("/api/v1/repos/:id/raw", get(repo_raw))
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
        .route("/repos", any(repo_entry))
        .route("/repos/*rest", any(repo_entry))
        // Steering: first-contact doc for a confused agent on the REST edge.
        .route("/.jeryu/agents/first-contact", any(github_forward))
        // Git smart-HTTP transport on the unified listener so `git clone`/`push`
        // work against this server. Mounted under `/git/` to stay clear of the
        // GitHub-shaped REST routes above: a root-level `:owner` param would
        // conflict with the literal `/repos`, `/users`, ... routes in the matcher.
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
mod workcell_surface_tests;
