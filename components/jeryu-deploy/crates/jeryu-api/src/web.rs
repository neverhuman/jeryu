//! Axum HTTP/WebSocket edge for the local live Jeryu API.
#![allow(unused_imports)] // child modules (`http`, bootstrap, tests) import these through `use super::*`.

#[cfg(test)]
mod test_databases;

mod agent_runs;
pub(crate) mod auth;
mod ci_evidence;
mod codegraph;
mod control_plane;
mod ecosystem;
mod embedded_web;
mod http;
mod markdown;
mod mcp_backend;
mod permissions;
mod pulls;
mod repo_admin;
mod repositories;
mod repository_create;
mod request_id;
mod sessions;
mod surface;
mod tool_build;
mod tool_finder;
mod tool_registry;
mod tool_status_messages;
mod work;
mod workcells;
mod workcells_support;
mod ws;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Extension, Path as AxumPath, Request, State};
use axum::http::{HeaderName, HeaderValue, Method as HttpMethod, StatusCode, header};
use axum::middleware::{Next, from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response as AxumResponse};
use axum::routing::{any, get, post};
use axum::{Json, Router as AxumRouter};
use jeryu_codegraph::CodeGraphStore;
use jeryu_core::{AccountSummary, ForgeCore, UserRole};
use jeryu_jira::WorkStore;
use jeryu_readmodel::TuiReadModel;
use jeryu_readmodel::contracts::{RepositoryRole, ServerWsMessage, WebEvent};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedSender;

use crate::GithubRouter;
use crate::git_materializer::GitMaterializer;
use crate::github::{
    GH_AUTH_BOUNDARY, GH_SETUP_COMMAND, GH_SETUP_TOKEN_FILE, MCP_GUIDANCE_TOOLS, MCP_RUN_TESTS_TOOL,
};
use jeryu_gitd::{GitdConfig, RepoManager};
use jeryu_runner_oci::{CliContainerRuntime, ContainerLifecycle};
use jeryu_runnerd::{WarmPool, WorkcellManager};
use repositories::{
    fleet_tool_adoption, repo_blob, repo_detail, repo_jankurai_scores_ingest,
    repo_jankurai_scores_list, repo_raw, repo_readme, repo_readme_update, repo_refs, repo_tree,
    repo_update, repos,
};
use surface::{bootstrap_payload_for_user, github_forward, graphql, markdown_render, repo_entry};

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
const BOOTSTRAP_ADMIN_LOGIN: &str = "jeryu-admin";
const BOOTSTRAP_ADMIN_PASSWORD_ENV: &str = "JERYU_BOOTSTRAP_ADMIN_PASSWORD";

#[derive(Clone, Debug)]
pub struct WebServerConfig {
    pub bind: SocketAddr,
    pub spa_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Storage root for bare git repositories served over smart-HTTP.
    pub git_storage_root: PathBuf,
    /// Optional split-family manifests used to classify portal/member repos.
    pub split_manifests: Vec<PathBuf>,
    /// Enforce account/session auth on `/api/v1/*`.
    pub auth_required: bool,
    /// Explicit single-host development bypass for local demos/tests.
    pub trust_local_dev: bool,
    /// Use Secure `__Host-` cookies. Disable only for plain-HTTP local dev.
    pub secure_cookies: bool,
}

mod catalog;

use catalog::{SplitCatalog, resolve_tool_registry_path};

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
    /// Local-first Work Tracker store shared by Work routes and the issue bridge.
    pub(crate) work: WorkStore,
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
    split_catalog: SplitCatalog,
    /// Path to `jeryu-tool/tools-registry.toml`, resolved from the split
    /// manifest in `serve()`. `None` in tests and when no manifest is wired, in
    /// which case the golden-box endpoint reports an empty registry.
    tool_registry_path: Option<PathBuf>,
    /// Split manifests handed to `serve()`; the tool-finder system scan
    /// derives its family-discovery parents from these. Empty in tests.
    split_manifests: Vec<PathBuf>,
    /// Single-flight state for the system-wide tool-finder scan, retained
    /// across scans so the page can paint the last result.
    pub(crate) tool_finder_scan: tool_finder::ToolFinderScanState,
    pub(crate) auth_required: bool,
    pub(crate) trust_local_dev: bool,
    pub(crate) secure_cookies: bool,
    pub(crate) auth_rate_limits: Arc<Mutex<BTreeMap<String, auth::RateLimitBucket>>>,
    #[cfg(test)]
    _test_databases: Arc<test_databases::TestDatabases>,
    /// Unit-test sessions must never seed agent credentials from the operator home.
    #[cfg(test)]
    session_auth_home: Arc<tempfile::TempDir>,
}

impl WebState {
    fn with_repo_manager(
        core: ForgeCore,
        repo_manager: Arc<RepoManager>,
        spa_dir: PathBuf,
        data_dir: PathBuf,
        split_catalog: SplitCatalog,
    ) -> Self {
        // Assemble a LIVE read model from ForgeCore state so the TUI/web panes
        // render real pool activity and system health, not the empty fixture.
        let tui = crate::read_model::assemble_read_model(&core);
        // ForgeCore is Arc-backed, so this handle shares state with `github`.
        let core_handle = core.clone();
        #[cfg(test)]
        let test_databases = Arc::new(test_databases::TestDatabases::scratch());
        let codegraph_path = {
            #[cfg(test)]
            {
                let _ = &data_dir;
                test_databases.codegraph_path()
            }
            #[cfg(not(test))]
            {
                data_dir.join("codegraph.sqlite")
            }
        };
        let codegraph_store = CodeGraphStore::open(codegraph_path).expect("open codegraph store");
        let work_path = {
            #[cfg(test)]
            {
                test_databases.work_path().to_path_buf()
            }
            #[cfg(not(test))]
            {
                data_dir.join("work.sqlite")
            }
        };
        let work = WorkStore::open(work_path).expect("open work store");
        // Pre-warm the agent pool over the real CLI lifecycle. With the OCI gate
        // closed this only records planned cells (no daemon), so construction is
        // infallible in every environment the web edge boots in.
        let warm_runtime: Arc<dyn ContainerLifecycle> = Arc::new(CliContainerRuntime);
        let warm_pool = Arc::new(Mutex::new(
            WarmPool::new(warm_runtime, WARM_POOL_TARGET).expect("pre-warm the agent pool"),
        ));
        Self {
            github: GithubRouter::with_core(core)
                .with_repo_manager(repo_manager.clone())
                .with_work_store(work.clone()),
            tui,
            spa_dir,
            ws: WsHub::new(),
            workcells: Arc::new(Mutex::new(WorkcellManager::new())),
            agent_runs: agent_runs::AgentRunStore::new(),
            codegraph_store,
            repo_manager,
            core: core_handle,
            work,
            warm_pool,
            session_runtime: sessions::SessionRuntimeConfig::from_env(),
            #[cfg(test)]
            session_auth_home: Arc::new(tempfile::tempdir().expect("session fixture auth home")),
            split_catalog,
            tool_registry_path: None,
            split_manifests: Vec::new(),
            tool_finder_scan: tool_finder::ToolFinderScanState::default(),
            auth_required: false,
            trust_local_dev: true,
            secure_cookies: false,
            auth_rate_limits: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(test)]
            _test_databases: test_databases,
        }
    }

    fn with_auth(mut self, required: bool, trust_local_dev: bool, secure_cookies: bool) -> Self {
        self.auth_required = required;
        self.trust_local_dev = trust_local_dev;
        self.secure_cookies = secure_cookies;
        self
    }

    /// Point the golden-box endpoint at `jeryu-tool/tools-registry.toml`.
    /// Production-only chaining in `serve()`; tests leave it unset.
    fn with_tool_registry_path(mut self, path: Option<PathBuf>) -> Self {
        self.tool_registry_path = path;
        self
    }

    /// Hand the tool-finder the split manifests so the system scan can derive
    /// its family-discovery parents. Production-only chaining in `serve()`.
    fn with_split_manifests(mut self, manifests: Vec<PathBuf>) -> Self {
        self.split_manifests = manifests;
        self
    }

    /// Attach the merge-to-GitHub mirror (loaded from the split manifest) to
    /// the embedded GitHub router. Production-only chaining in `serve()`;
    /// every other constructor leaves the mirror absent, so no test or
    /// embedded caller ever attempts a network push.
    fn with_github_mirror(mut self, mirror: Arc<crate::github_mirror::GithubMirror>) -> Self {
        self.github = self.github.with_github_mirror(mirror);
        self
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
            std::env::temp_dir(),
            SplitCatalog::builtin(),
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
            std::env::temp_dir(),
            SplitCatalog::builtin(),
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
/// Hands out the server-wide monotonic event sequence, tracks which scopes
/// each live connection is subscribed to, and fans producer events out to
/// exactly the interested connections through their registered outbound
/// queues ([`WsHub::publish`]). The snapshot-on-subscribe path also rides
/// this hub.
#[derive(Clone, Default)]
struct WsHub {
    inner: Arc<Mutex<WsHubInner>>,
}

#[derive(Default)]
struct WsHubInner {
    /// Server-wide monotonic event sequence; never reused, never decreases.
    next_seq: u64,
    /// Dedicated connection-id counter (never reused).
    next_conn_id: u64,
    /// Live connections, in registration order. Each tracks its own scopes.
    connections: Vec<WsConnection>,
}

/// A single live WebSocket connection's subscription state inside the hub.
struct WsConnection {
    id: u64,
    scopes: BTreeSet<String>,
    /// Outbound push lane drained by the connection's socket loop.
    sender: UnboundedSender<ServerWsMessage>,
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

    /// Register a fresh connection (with its outbound queue) and return its
    /// hub-unique id.
    fn register(&self, sender: UnboundedSender<ServerWsMessage>) -> u64 {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        inner.next_conn_id = inner.next_conn_id.saturating_add(1);
        let id = inner.next_conn_id;
        inner.connections.push(WsConnection {
            id,
            scopes: BTreeSet::new(),
            sender,
        });
        id
    }

    /// Allocate a sequence, build the event once, and queue an `Event` frame
    /// to every connection subscribed to `scope`. Connections whose socket
    /// loop has gone away (receiver dropped) are pruned. Returns how many
    /// connections the event was queued to. Safe to call from blocking
    /// threads: `UnboundedSender::send` never blocks.
    fn publish(&self, scope: &str, make_event: impl FnOnce(u64) -> WebEvent) -> usize {
        let mut inner = self.inner.lock().expect("ws hub mutex poisoned");
        inner.next_seq = inner.next_seq.saturating_add(1);
        let frame = ServerWsMessage::Event {
            event: make_event(inner.next_seq),
        };
        let mut delivered = 0;
        inner.connections.retain(|conn| {
            if !conn.scopes.contains(scope) {
                return true;
            }
            match conn.sender.send(frame.clone()) {
                Ok(()) => {
                    delivered += 1;
                    true
                }
                Err(_) => false,
            }
        });
        delivered
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
    if config.trust_local_dev && !config.bind.ip().is_loopback() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "trust_local_dev requires a loopback bind address",
        )));
    }
    let db_path = config.data_dir.join("forge.sqlite");
    // Share one RepoManager between the create-repo materializer (so a created
    // repo gets a bare repo on disk) and the smart-HTTP transport (so it can be
    // cloned/pushed) — both rooted at the same git storage root.
    let repo_manager = Arc::new(RepoManager::new(GitdConfig::new(
        config.git_storage_root.clone(),
    )));
    let core = ForgeCore::open_managed(&db_path, &config.git_storage_root)?
        .with_repo_materializer(Arc::new(GitMaterializer::new(repo_manager.clone())));
    let split_catalog = SplitCatalog::load(&config.split_manifests);
    let tool_registry_path = resolve_tool_registry_path(&config.split_manifests);
    // Merge-to-GitHub mirroring: targets come from the same manifest; with no
    // manifest (or JERYU_GITHUB_PUSH=0) the mirror loads disabled and merges
    // never attempt a push.
    let github_mirror = Arc::new(crate::github_mirror::GithubMirror::load(
        &config.split_manifests,
    ));
    let state = WebState::with_repo_manager(
        core,
        repo_manager,
        config.spa_dir.clone(),
        config.data_dir.clone(),
        split_catalog,
    )
    .with_github_mirror(github_mirror)
    .with_tool_registry_path(tool_registry_path)
    .with_split_manifests(config.split_manifests.clone())
    .with_auth(
        config.auth_required,
        config.trust_local_dev,
        config.secure_cookies,
    );
    bootstrap_public_accounts(&state, &config.data_dir)?;
    let app = app(state, &config.spa_dir);
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

mod bootstrap;
#[cfg(test)]
mod bootstrap_tests;

use bootstrap::bootstrap_public_accounts;
#[cfg(test)]
use bootstrap::bootstrap_public_accounts_with_admin_password;

#[cfg(test)]
use http::{
    HDR_API, HDR_FAST_PATH, HDR_TOOL, advisory_headers, bootstrap_tui, capabilities_payload,
    is_automation_agent, suggested_tool,
};
use http::{api_error, app, server_time};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod agent_runs_tests;

#[cfg(test)]
mod workcell_surface_tests;
