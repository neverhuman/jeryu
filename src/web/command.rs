//! `jeryu web serve` — Phase-1 BFF entry point (W-B-01..05).
//!
//! Wires the Axum + WebSocket + SPA static-asset binding. The legacy
//! engine routes (`/health`, `/hooks`, `/cache/summary`) are preserved
//! verbatim by merging `engine::build_router(state)` into the new
//! `/api/v1/*` router per `WEB_WORK_CLAUDE.md` §35.1.5.
//!
//! Phase 1 boots a minimal `EngineState` (in-memory SQLite, disconnected
//! Docker, no-op GitLab client) so the legacy routes survive without
//! requiring a full operator-provisioned environment. Production
//! deployments will compose this entry point with the real `EngineState`
//! once the service layer (W-B-06+) lands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::info;

use crate::cli::WebCommand;

pub(crate) async fn run(cmd: WebCommand) -> Result<()> {
    match cmd {
        WebCommand::Serve {
            bind,
            open,
            dev_assets,
            spa_dir,
        } => serve(&bind, open, dev_assets.as_deref(), &spa_dir).await,
        WebCommand::Open => {
            println!("jeryu web open (stub): would print the bound URL");
            Ok(())
        }
        WebCommand::BuildAssets => {
            println!(
                "jeryu web build-assets (stub): delegates to `npm --workspace @jeryu/web run build`"
            );
            Ok(())
        }
        WebCommand::ExportSchemas { out_dir } => {
            #[cfg(feature = "web")]
            {
                let (openapi_bytes, ws_bytes) = crate::web::openapi::export_schemas(&out_dir)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                eprintln!(
                    "wrote {} ({} bytes)",
                    out_dir.join("web-api.openapi.json").display(),
                    openapi_bytes,
                );
                eprintln!(
                    "wrote {} ({} bytes)",
                    out_dir.join("websocket-events.schema.json").display(),
                    ws_bytes,
                );
                Ok(())
            }
            #[cfg(not(feature = "web"))]
            {
                let _ = out_dir;
                Err(anyhow::anyhow!("export-schemas requires --features web"))
            }
        }
    }
}

async fn serve(bind: &str, open: bool, dev_assets: Option<&str>, spa_dir: &str) -> Result<()> {
    use jeryu::docker::DockerCtl;
    use jeryu::engine::{self, EngineState};
    use jeryu::gitlab_client::GitlabClient;
    use jeryu::runner_backend_registry::BackendRegistry;
    use jeryu::state::Db;
    use jeryu::web_events::WebEventBus;

    use crate::web::{router::build_web_router, state::WebState};

    if let Some(url) = dev_assets {
        // Phase 1 does not implement the reverse-proxy; the dev workflow
        // expects the user to run Vite separately and point their browser
        // there. We surface the misconfiguration loudly instead of
        // silently serving 502s.
        tracing::warn!(
            dev_assets = url,
            "--dev-assets is not implemented in Phase 1; run `npm --workspace @jeryu/web run dev` separately"
        );
    }

    // ── Build the legacy engine router ────────────────────────────────
    // Phase 1 boots with a minimal in-memory state so /health, /hooks,
    // /cache/summary remain bound per §35.1.5. The real engine (W-B-*)
    // will compose this entry point with its provisioned state.
    let db = Db::open_memory()
        .await
        .context("opening in-memory SQLite for the engine state")?;
    let session_pool = db.pool();
    let docker = DockerCtl::disconnected();
    let client = GitlabClient::new("http://127.0.0.1:0", None);
    let backend_registry = BackendRegistry::local_only(docker.clone());
    let engine_state = Arc::new(EngineState {
        db,
        docker,
        client,
        webhook_secret: String::new(),
        backend_registry,
        node_gc_timestamps: Mutex::new(HashMap::new()),
    });
    let legacy_router = engine::build_router(engine_state);

    // ── Build the Web Forge BFF state ─────────────────────────────────
    let event_bus = Arc::new(WebEventBus::with_defaults());
    let web_state = WebState::new_for_serve(event_bus, session_pool);

    // ── Compose and bind ──────────────────────────────────────────────
    let router = build_web_router(web_state, legacy_router, spa_dir);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    let local_addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| bind.to_string());
    info!(addr = %local_addr, spa_dir, "jeryu web serve listening");
    println!("jeryu web serve listening on http://{local_addr}");
    println!("  spa-dir:      {spa_dir}");
    println!("  bootstrap:    http://{local_addr}/api/v1/bootstrap");
    println!("  websocket:    ws://{local_addr}/api/v1/ws");
    println!("  legacy:       /health /hooks /cache/summary");

    if open {
        let url = format!("http://{local_addr}/");
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = open_url(&url);
        });
    }

    axum::serve(listener, router)
        .await
        .context("axum::serve exited")?;
    Ok(())
}

/// Best-effort `xdg-open` invocation. Failure is logged but never fatal.
fn open_url(url: &str) -> Result<()> {
    let status = std::process::Command::new("xdg-open").arg(url).status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            tracing::warn!(url, exit = ?s, "xdg-open exited non-zero");
            Ok(())
        }
        Err(e) => {
            tracing::warn!(url, error = %e, "xdg-open not available");
            Ok(())
        }
    }
}
