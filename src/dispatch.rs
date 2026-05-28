//! Owner: CLI Dispatch
//! Proof: `cargo check -p jeryu`
//! Invariants: All logic dispatches to domain modules; no business logic here
//!
//! Wires CLI commands to domain module functions.

use anyhow::Result;

use crate::cli::*;
use jeryu::*;

#[path = "dispatch_back.rs"]
mod dispatch_back;
#[path = "dispatch_support.rs"]
mod dispatch_support;

pub use dispatch_support::fetch_ci_job_runs;

// ---------------------------------------------------------------------------
// Helpers

fn env_var_or_empty(name: &str) -> String {
    if let Ok(value) = std::env::var(name) {
        return value;
    }
    match gitlab_auth::load_env_value(name) {
        Ok(Some(value)) => value,
        Ok(None) | Err(_) => String::new(),
    }
}

/// Load secrets from jeryu.env and build a GitlabClient.
pub async fn load_client() -> Result<(gitlab_client::GitlabClient, String)> {
    let auth = gitlab_auth::resolve_or_repair_default().await?;
    let webhook_secret = env_var_or_empty("JERYU_WEBHOOK_SECRET");

    let client = gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));

    Ok((client, webhook_secret))
}

fn load_client_optional() -> (gitlab_client::GitlabClient, String) {
    let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    let pat = gitlab_auth::load_token_for_url(&url).ok().flatten();
    let webhook_secret = env_var_or_empty("JERYU_WEBHOOK_SECRET");

    let client = gitlab_client::GitlabClient::new(&url, pat);

    (client, webhook_secret)
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub(crate) async fn run(cli: Cli) -> Result<i32> {
    match cli.command {
        // ---- Init --------------------------------------------------------
        Commands::Init | Commands::Bootstrap => {
            bootstrap::run_bootstrap().await?;
        }

        // ---- Serve -------------------------------------------------------
        Commands::Serve => {
            jeryu::repo_fleet::ensure_workspace_root_default();
            let (client, webhook_secret) = load_client().await?;
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;

            // Ensure GitLab is running
            docker_ctl.compose_up().await?;

            // Install the Agent OS Admission plane (global server hooks)
            if let Err(e) = admission::install_global_hook() {
                tracing::warn!("Failed to install global server hook: {}", e);
            }

            // Start SmartCache supervisor
            cache::SmartCache::new(db.clone()).start().await?;

            let repaired_pools = pool::ensure_default_pool_rows(&db, &client).await?;
            if repaired_pools > 0 {
                tracing::warn!(
                    repaired_pools,
                    "repaired missing default runner pool rows before reconciliation"
                );
            }

            let pools = db.list_pools().await?;
            let normalized_runners =
                jeryu::runner_policy::enforce_pool_runner_policy(&client, &pools).await?;
            if normalized_runners > 0 {
                tracing::warn!(
                    normalized_runners,
                    "normalized GitLab runners to pool policy before reconciliation"
                );
            }

            // Reconcile every pool to min_warm, including zero-warm pools.
            // This drains outdated ad hoc managers instead of leaving them alive
            // indefinitely between serve restarts.
            for p in &pools {
                if !p.paused {
                    pool::scale_pool_to(&db, &docker_ctl, &client, &p.name, p.min_warm as usize)
                        .await?;
                }
            }

            println!("✅ All pools at min_warm. Starting background engine...");

            let db_clone = db.clone();
            let docker_clone = docker_ctl.clone();
            let client_clone = client.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    engine::run_engine(db_clone, docker_clone, client_clone, webhook_secret).await
                {
                    tracing::error!("Engine error: {:?}", e);
                }
            });

            // Wait for signal to exit
            tokio::signal::ctrl_c().await?;
            println!("\nShutting down engine...");
        }

        // ---- Install ----------------------------------------------------
        Commands::Install(subcmd) => {
            return crate::commands::install::execute_install_command(subcmd).await;
        }

        // ---- Remote -----------------------------------------------------
        Commands::Remote(subcmd) => {
            return crate::commands::remote::execute_remote_command(subcmd).await;
        }

        // ---- Tui ---------------------------------------------------------
        Commands::Tui {
            once,
            demo,
            capture,
            screenshot,
            tab,
            output,
            width,
            height,
            screenshot_hold_ms,
        } => {
            jeryu::repo_fleet::ensure_workspace_root_default();
            let (client, _) = if once || capture || screenshot || demo {
                load_client_optional()
            } else {
                load_client().await?
            };
            let docker_ctl = if once || capture || screenshot || demo {
                // Screenshot/capture/demo modes never interact with Docker.
                match docker::DockerCtl::connect() {
                    Ok(ctl) => ctl,
                    Err(_) => docker::DockerCtl::disconnected(),
                }
            } else {
                docker::DockerCtl::connect()?
            };

            if capture {
                jeryu::tui::capture_tui_png(None, docker_ctl, client, &tab, &output, width, height)
                    .await?;
                println!("jeryu TUI screenshot written: {}", output.display());
            } else if screenshot {
                jeryu::tui::run_tui_screenshot(None, docker_ctl, client, &tab, screenshot_hold_ms)
                    .await?;
            } else if once {
                jeryu::tui::run_tui_once(None, docker_ctl, client, &tab).await?;
            } else if demo {
                jeryu::tui::run_tui(None, docker_ctl, client, &tab, true).await?;
            } else {
                let db = state::Db::open().await?;
                // Start TUI (blocks until exit)
                jeryu::tui::run_tui(Some(db), docker_ctl, client, &tab, false).await?;
            }
        }

        // ---- Git Operations ----------------------------------------------
        Commands::Git { args } => {
            let db = state::Db::open().await.ok();
            return crate::commands::git::execute_git_passthrough(db.as_ref(), &args).await;
        }
        Commands::Save { message } => {
            let db = state::Db::open().await.ok();
            return crate::commands::git::execute_save(db.as_ref(), &message).await;
        }
        Commands::Sync => {
            let db = state::Db::open().await.ok();
            return crate::commands::git::execute_sync(db.as_ref()).await;
        }
        Commands::Undo => {
            let db = state::Db::open().await.ok();
            return crate::commands::git::execute_undo(db.as_ref()).await;
        }
        // ---- Down --------------------------------------------------------
        Commands::Down => crate::commands::system::execute_down().await?,

        // ---- Status (Native wrapper) -------------------------------------
        Commands::Status => crate::commands::system::execute_status()?,

        // ---- System (formerly Status) ------------------------------------
        Commands::System => crate::commands::system::execute_system_status().await?,

        // ---- Pool --------------------------------------------------------
        Commands::Pool(subcmd) => crate::commands::pool::execute_pool_commands(subcmd).await?,

        // ---- Job ---------------------------------------------------------
        Commands::Job(subcmd) => crate::commands::job::execute_job_commands(subcmd).await?,

        Commands::Pipeline(subcmd) => {
            crate::commands::pipeline::execute_pipeline_commands(subcmd).await?
        }

        Commands::Bug(subcmd) => return crate::commands::bug::execute_bug_commands(subcmd).await,

        // ---- Web Forge BFF (Phase-0 stub; W-F-10) ------------------------
        // Real Axum binding lands in W-B-01..04 per WEB_WORK_CLAUDE.md §7.0.
        // Legacy routes /health, /hooks, /cache/summary are preserved by
        // W-B-02 per WEB_WORK_CLAUDE.md §35.1.5.
        Commands::Web(cmd) => crate::web::command::run(cmd).await?,

        other => return dispatch_back::run(other).await,
    }

    Ok(0)
}
