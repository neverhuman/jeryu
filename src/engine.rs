//! Owner: Engine Core (Webhook + Reconciliation)
//! Proof: `cargo test -p jeryu -- engine`
//! Invariants: 5-min recon cycle; Docker crash recovery via event stream; supersedence on newer SHA
//!
//! The engine is the real-time brain. It runs two concurrent tasks:
//! 1. An Axum HTTP server that receives GitLab webhook events
//! 2. A periodic reconciliation loop that syncs desired vs actual state

use anyhow::Result;
use axum::{
    Router,
    routing::{get, post},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::info;

use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::runner_backend_registry::BackendRegistry;
use crate::state::Db;

#[path = "engine_aux.rs"]
mod aux_secondary;
#[path = "engine_background.rs"]
mod background;
#[path = "engine_webhook.rs"]
mod webhook;

pub(crate) use background::{
    cache_summary, check_scale_up, docker_event_loop, reconciliation_loop, system_health_loop,
};
pub(crate) use webhook::dispatch_inline;
pub(crate) use webhook::{handle_webhook, health};

// ---------------------------------------------------------------------------
// Shared state for the engine
// ---------------------------------------------------------------------------

pub struct EngineState {
    pub db: Db,
    /// Local Docker control (used for GitLab/Vault/compose management).
    pub docker: DockerCtl,
    pub client: GitlabClient,
    pub webhook_secret: String,
    /// Registry of all runner backends (local, remote SSH nodes, K8s clusters).
    pub backend_registry: BackendRegistry,
    /// Tracks when each remote node last had its storage GC'd.
    /// Key = node alias, value = Instant of last GC.
    pub node_gc_timestamps: Mutex<HashMap<String, Instant>>,
}

pub type SharedState = Arc<EngineState>;

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

/// Build the engine's HTTP router with the legacy routes wired to the supplied
/// shared state.
///
/// The three routes registered here — `/health`, `/hooks`, `/cache/summary` —
/// form the legacy engine surface that **must** be preserved when the Web Forge
/// BFF lands (see `WEB_WORK_CLAUDE.md` §35.1.5 and W-B-02). Tests assert these
/// routes remain bound; W-F-13 owns the regression coverage in
/// `tests/engine_routes_preserved_test.rs`.
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/hooks", post(handle_webhook))
        .route("/cache/summary", get(cache_summary))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Engine entry point
// ---------------------------------------------------------------------------

/// Start the engine (webhook server + reconciliation loop).
/// This runs indefinitely until the process is killed.
pub async fn run_engine(
    db: Db,
    docker: DockerCtl,
    client: GitlabClient,
    webhook_secret: String,
) -> Result<()> {
    let backend_registry = BackendRegistry::build(docker.clone());
    let state = Arc::new(EngineState {
        db,
        docker,
        client,
        webhook_secret,
        backend_registry,
        node_gc_timestamps: Mutex::new(HashMap::new()),
    });

    // Build router (see `build_router` above for the §35.1.5 invariant).
    let app = build_router(state.clone());

    // Start reconciliation loop
    let reconcile_state = state.clone();
    tokio::spawn(async move {
        reconciliation_loop(reconcile_state).await;
    });

    // Start pipeline-cache self-healing loop: reconciles tracked pipelines
    // against live GitLab so a dropped status webhook never leaves a phantom
    // "created" pipeline in `jeryu next` / the TUI.
    let pipeline_reconcile_state = state.clone();
    tokio::spawn(async move {
        crate::pipeline_reconcile::reconcile_loop(pipeline_reconcile_state).await;
    });

    // Start Docker event listener loop (makes scaling instant)
    let event_state = state.clone();
    tokio::spawn(async move {
        docker_event_loop(event_state).await;
    });

    let addr = crate::settings::get().webhook.bind.clone();
    info!(addr = %addr, "starting jeryu engine");

    // Start background health sentinel loop
    let health_state = state.clone();
    tokio::spawn(async move {
        system_health_loop(health_state).await;
    });

    // Bring up the selected message log and the consumer loop that drains
    // webhook events into the dispatch path. If init fails the engine still
    // serves HTTP, but webhooks reject with 503 until the operator restarts.
    #[cfg(any(feature = "kafka-backend", feature = "jansu-broker"))]
    {
        match crate::messaging::init_message_log().await {
            Ok(message_log) => {
                let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                // Leak the sender deliberately — the consumer loop runs for the
                // lifetime of the engine and there's no external cancellation
                // path until the engine itself is dropped.
                std::mem::forget(_shutdown_tx);
                let consumer_state = state.clone();
                tokio::spawn(async move {
                    crate::messaging::consumer_loop::spawn(
                        consumer_state,
                        message_log,
                        shutdown_rx,
                    )
                    .await
                    .ok();
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "message log init failed");
            }
        }
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
