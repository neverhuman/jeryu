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
use std::sync::Arc;
use tracing::info;

use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
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
#[cfg(feature = "jansu-broker")]
pub(crate) use webhook::dispatch_inline;
pub(crate) use webhook::{handle_webhook, health};

// ---------------------------------------------------------------------------
// Shared state for the engine
// ---------------------------------------------------------------------------

pub struct EngineState {
    pub db: Db,
    pub docker: DockerCtl,
    pub client: GitlabClient,
    pub webhook_secret: String,
}

pub type SharedState = Arc<EngineState>;

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
    let state = Arc::new(EngineState {
        db,
        docker,
        client,
        webhook_secret,
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/hooks", post(handle_webhook))
        .route("/cache/summary", get(cache_summary))
        .with_state(state.clone());

    // Start reconciliation loop
    let reconcile_state = state.clone();
    tokio::spawn(async move {
        reconciliation_loop(reconcile_state).await;
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

    // Bring up the embedded jansu broker and the consumer loop that drains
    // webhook events into the dispatch path. If broker init fails the engine
    // still serves HTTP, but webhooks reject with 503 until the operator restarts.
    #[cfg(feature = "jansu-broker")]
    {
        match crate::messaging::init_broker().await {
            Ok(broker) => {
                let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                // Leak the sender deliberately — the consumer loop runs for the
                // lifetime of the engine and there's no external cancellation
                // path until the engine itself is dropped.
                std::mem::forget(_shutdown_tx);
                let consumer_state = state.clone();
                tokio::spawn(async move {
                    crate::messaging::consumer_loop::spawn(consumer_state, broker, shutdown_rx)
                        .await
                        .ok();
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "embedded jansu broker init failed");
            }
        }
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
