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

// ---------------------------------------------------------------------------
// Helpers

fn env_var_or_empty(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) | Err(std::env::VarError::NotUnicode(_)) => {
            String::new()
        }
    }
}

/// Load secrets from jeryu.env and build a GitlabClient.
pub fn load_client() -> Result<(gitlab_client::GitlabClient, String)> {
    let env_path = config::env_file();
    dotenvy::from_path(&env_path).ok();

    let pat = std::env::var("GITLAB_PAT")
        .map_err(|_| anyhow::anyhow!("GITLAB_PAT not found — run `jeryu bootstrap` first"))?;
    let webhook_secret = env_var_or_empty("JERYU_WEBHOOK_SECRET");

    let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    let client = gitlab_client::GitlabClient::new(&url, Some(pat));

    Ok((client, webhook_secret))
}

fn load_client_optional() -> (gitlab_client::GitlabClient, String) {
    let env_path = config::env_file();
    dotenvy::from_path(&env_path).ok();

    let pat = std::env::var("GITLAB_PAT").ok();
    let webhook_secret = env_var_or_empty("JERYU_WEBHOOK_SECRET");

    let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    let client = gitlab_client::GitlabClient::new(&url, pat);

    (client, webhook_secret)
}

pub async fn fetch_ci_job_runs(
    client: &gitlab_client::GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<Vec<state::CiJobRun>> {
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    let observed_at = chrono::Utc::now().to_rfc3339();
    Ok(jobs
        .into_iter()
        .map(|job| {
            let runner = job.runner.and_then(|runner| runner.description);
            let actual_pipeline_id = job.pipeline_id.unwrap_or(pipeline_id);
            state::CiJobRun {
                job_id: job.id,
                project_id,
                pipeline_id: actual_pipeline_id,
                root_pipeline_id: pipeline_id,
                pipeline_sha: pipeline.sha.clone(),
                ref_name: pipeline.ref_name.clone(),
                job_name: job.name,
                stage: job.stage,
                status: job.status,
                runner_pool: runner
                    .as_deref()
                    .and_then(infer_runner_pool)
                    .map(str::to_string),
                runner,
                queued_duration_secs: job.queued_duration,
                duration_secs: job.duration,
                started_at: job.started_at,
                finished_at: job.finished_at,
                web_url: job.web_url,
                observed_at: observed_at.clone(),
            }
        })
        .collect())
}

fn infer_runner_pool(runner: &str) -> Option<&'static str> {
    let lower = runner.to_ascii_lowercase();
    if lower.contains("untrusted") {
        Some("untrusted")
    } else if lower.contains("build") {
        Some("build")
    } else if lower.contains("default") {
        Some("default")
    } else {
        None
    }
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
            let (client, webhook_secret) = load_client()?;
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

            // Reconcile every pool to min_warm, including zero-warm pools.
            // This drains outdated ad hoc managers instead of leaving them alive
            // indefinitely between serve restarts.
            let pools = db.list_pools().await?;
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
            let (client, _) = if once || capture || screenshot || demo {
                load_client_optional()
            } else {
                load_client()?
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

        other => return dispatch_back::run(other).await,
    }

    Ok(0)
}
