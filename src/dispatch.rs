//! Owner: CLI Dispatch
//! Proof: `cargo check -p jeryu`
//! Invariants: All logic dispatches to domain modules; no business logic here
//!
//! Wires CLI commands to domain module functions.

use anyhow::Result;
use std::path::PathBuf;

use crate::cli::*;
use jeryu::*;

// ---------------------------------------------------------------------------
// Helpers

/// Load secrets from jeryu.env and build a GitlabClient.
fn load_client() -> Result<(gitlab_client::GitlabClient, String)> {
    let env_path = config::env_file();
    dotenvy::from_path(&env_path).ok();

    let pat = std::env::var("GITLAB_PAT")
        .map_err(|_| anyhow::anyhow!("GITLAB_PAT not found — run `jeryu bootstrap` first"))?;
    let webhook_secret = std::env::var("JERYU_WEBHOOK_SECRET").unwrap_or_default();

    let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    let client = gitlab_client::GitlabClient::new(&url, Some(pat));

    Ok((client, webhook_secret))
}

fn load_client_optional() -> (gitlab_client::GitlabClient, String) {
    let env_path = config::env_file();
    dotenvy::from_path(&env_path).ok();

    let pat = std::env::var("GITLAB_PAT").ok();
    let webhook_secret = std::env::var("JERYU_WEBHOOK_SECRET").unwrap_or_default();

    let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    let client = gitlab_client::GitlabClient::new(&url, pat);

    (client, webhook_secret)
}

async fn fetch_ci_job_runs(
    client: &gitlab_client::GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<Vec<state::CiJobRun>> {
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let jobs = client.list_pipeline_jobs(project_id, pipeline_id).await?;
    let observed_at = chrono::Utc::now().to_rfc3339();
    Ok(jobs
        .into_iter()
        .map(|job| {
            let runner = job.runner.and_then(|runner| runner.description);
            state::CiJobRun {
                job_id: job.id,
                project_id,
                pipeline_id,
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

pub(crate) async fn run(cli: Cli) -> Result<()> {
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
            // This drains stale ad hoc managers instead of leaving them alive
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

            let db_clone2 = db.clone();
            let client_clone2 = client.clone();
            tokio::spawn(async move {
                shadow::run_shadow_loop(db_clone2, client_clone2).await;
            });

            // Wait for signal to exit
            tokio::signal::ctrl_c().await?;
            println!("\nShutting down engine...");
        }

        // ---- Tui ---------------------------------------------------------
        Commands::Tui {
            once,
            capture,
            screenshot,
            tab,
            output,
            width,
            height,
            screenshot_hold_ms,
        } => {
            let (client, _) = if once || capture || screenshot {
                load_client_optional()
            } else {
                load_client()?
            };
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;

            if capture {
                jeryu::tui::capture_tui_png(db, docker_ctl, client, &tab, &output, width, height)
                    .await?;
                println!("jeryu TUI screenshot written: {}", output.display());
            } else if screenshot {
                jeryu::tui::run_tui_screenshot(db, docker_ctl, client, &tab, screenshot_hold_ms)
                    .await?;
            } else if once {
                jeryu::tui::run_tui_once(db, docker_ctl, client).await?;
            } else {
                // Start TUI (blocks until exit)
                jeryu::tui::run_tui(db, docker_ctl, client).await?;
            }
        }

        // ---- Git Passthrough ---------------------------------------------
        Commands::Git { args } => {
            // Find system git but avoid infinite recursion if aliased.
            let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
            
            // Auto-Shadow: Intercept push command
            let is_push = args.first().map(|s| s.as_str()) == Some("push");
            
            let status = std::process::Command::new(&git_path)
                .args(&args)
                .status()?;
                
            if is_push && status.success() {
                // Determine if 'shadow' remote exists
                let remotes = std::process::Command::new(&git_path)
                    .args(["remote"])
                    .output();
                    
                if let Ok(out) = remotes {
                    let remote_str = String::from_utf8_lossy(&out.stdout);
                    if remote_str.lines().any(|l| l.trim() == "shadow") {
                        println!("🪄 JeRyu: Automatically pushing to local shadow pipeline...");
                        let _ = std::process::Command::new(&git_path)
                            .args(["push", "shadow", "HEAD"])
                            .status();
                    }
                }
            }
                
            std::process::exit(status.code().unwrap_or(1));
        }

        // ---- Save --------------------------------------------------------
        Commands::Save { message } => {
            println!("Saving work...");
            let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
            std::process::Command::new(&git_path).args(["add", "."]).status()?;
            let status = std::process::Command::new(&git_path).args(["commit", "-m", &message]).status()?;
            if !status.success() {
                println!("Failed to save changes.");
            } else {
                println!("✅ Work saved locally.");
            }
        }

        // ---- Sync --------------------------------------------------------
        Commands::Sync => {
            println!("Syncing with remote...");
            let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
            let pull_status = std::process::Command::new(&git_path).args(["pull", "--rebase"]).status()?;
            if pull_status.success() {
                let push_status = std::process::Command::new(&git_path).args(["push"]).status()?;
                if push_status.success() {
                    println!("✅ Synced successfully.");
                }
            }
        }

        // ---- Undo --------------------------------------------------------
        Commands::Undo => {
            println!("Undoing last save...");
            let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
            let status = std::process::Command::new(&git_path).args(["reset", "HEAD~1", "--soft"]).status()?;
            if status.success() {
                println!("✅ Last commit undone (changes kept in staging).");
            }
        }

        // ---- Ship --------------------------------------------------------
        Commands::Ship => {
            println!("Shipping code...");
            let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
            
            // Push to remote
            println!("Pushing to origin...");
            std::process::Command::new(&git_path).args(["push", "origin", "HEAD"]).status()?;
            
            // Push to local shadow
            println!("Promoting to local shadow runner...");
            let shadow_status = std::process::Command::new(&git_path)
                .args(["push", "shadow", "HEAD"])
                .status();
                
            match shadow_status {
                Ok(s) if s.success() => println!("✅ Shipped to remote and local shadow."),
                _ => println!("✅ Shipped to remote (local shadow skip/fail)."),
            }
        }

        // ---- Down --------------------------------------------------------
        Commands::Down => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;

            println!("Draining all pools...");
            let pools = db.list_pools().await?;
            for p in &pools {
                pool::drain_pool(&db, &docker_ctl, &client, &p.name)
                    .await
                    .ok();
                println!("  ✅ Pool '{}' drained", p.name);
            }

            println!("Stopping GitLab...");
            docker_ctl.compose_down().await?;
            println!("✅ Everything stopped.");
        }

        // ---- Status (Native wrapper) -------------------------------------
        Commands::Status => {
            println!("━━━ JeRyu Status ━━━\n");
            let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
            std::process::Command::new(&git_path).args(["status"]).status()?;
        }

        // ---- System (formerly Status) ------------------------------------
        Commands::System => {
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;

            // Check GitLab health
            let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
            let client = gitlab_client::GitlabClient::new(&url, None);
            let gitlab_ready = client.is_ready().await;

            println!("━━━ JeRyu system status ━━━\n");
            println!(
                "  GitLab:  {} ({})",
                if gitlab_ready {
                    "✅ running"
                } else {
                    "❌ not ready"
                },
                url,
            );
            match secrets::vault_status(Some(&db)).await {
                Ok(vault) => println!(
                    "  Vault:   {} ({})",
                    if vault.healthy {
                        "✅ ready"
                    } else if vault.sealed {
                        "⚠ sealed"
                    } else {
                        "❌ unavailable"
                    },
                    vault.addr
                ),
                Err(err) => println!("  Vault:   ❌ error ({err})"),
            }

            let pools = db.list_pools().await?;
            println!("  Pools:   {}", pools.len());

            for p in &pools {
                let active = db.count_active_managers(&p.name).await.unwrap_or(0);
                let running = pool::count_running_managers(&db, &docker_ctl, &p.name)
                    .await
                    .unwrap_or(0);
                let state_str = if p.paused { "⏸ paused" } else { "▶ active" };
                let manager_status = format!("{running}/{active}/{}", p.max_managers);
                println!(
                    "    {:<15} {} | managers: {} | runner_id: {}",
                    p.name, state_str, manager_status, p.gitlab_runner_id
                );
            }

            let managed = docker_ctl
                .list_managed_containers()
                .await
                .unwrap_or_default();
            println!("\n  Docker containers (jeryu-managed): {}", managed.len());
            for c in &managed {
                let name = c
                    .names
                    .as_ref()
                    .and_then(|n| n.first())
                    .map(|s| s.as_str())
                    .unwrap_or("?");
                let state = c.state.as_deref().unwrap_or("?");
                println!("    {} [{}]", name, state);
            }

            let events = db.recent_job_events(5).await.unwrap_or_default();
            if !events.is_empty() {
                println!("\n  Recent job events:");
                for e in &events {
                    println!(
                        "    job={:<6} project={:<4} status={:<10} {}",
                        e.job_id, e.project_id, e.status, e.received_at
                    );
                }
            }

            if let Ok(report) = release::build_release_status_report(
                &db,
                release::ReleaseStatusQuery {
                    project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                    ref_name: Some("main".into()),
                    sha: None,
                    limit: 1,
                },
            )
            .await
            {
                println!("\n  Latest release:");
                if let Some(latest) = report.latest {
                    println!("    {}", release::summarize_release_attempt(&latest));
                } else {
                    println!("    (none)");
                }
            }

            if let Ok(Some(secret_set)) = db.latest_release_secret_set("dougx").await {
                println!("\n  Latest release secret set:");
                println!(
                    "    {} {} [{}] {}",
                    secret_set.version,
                    secret_set.target,
                    secret_set.status,
                    secret_set.authority_name
                );
            }

            println!();
        }

        // ---- Pool --------------------------------------------------------
        Commands::Pool(subcmd) => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;

            match subcmd {
                PoolCommands::List => {
                    let pools = db.list_pools().await?;
                    println!(
                        "{:<15} {:<8} {:<10} {:<8} {:<12} {:<8}",
                        "NAME", "PAUSED", "EXECUTOR", "WARM", "LIVE/DB/MAX", "RUNNER"
                    );
                    for p in &pools {
                        let active = db.count_active_managers(&p.name).await.unwrap_or(0);
                        let running = pool::count_running_managers(&db, &docker_ctl, &p.name)
                            .await
                            .unwrap_or(0);
                        let manager_status = format!("{running}/{active}/{}", p.max_managers);
                        println!(
                            "{:<15} {:<8} {:<10} {:<8} {:<12} {:<8}",
                            p.name,
                            if p.paused { "yes" } else { "no" },
                            p.executor,
                            p.min_warm,
                            manager_status,
                            p.gitlab_runner_id,
                        );
                    }
                }
                PoolCommands::Scale { name, count } => {
                    let started =
                        pool::scale_pool_to(&db, &docker_ctl, &client, &name, count).await?;
                    println!(
                        "✅ Pool '{}' scaled to {} (started {} new)",
                        name, count, started
                    );
                }
                PoolCommands::Pause { name } => {
                    pool::pause_pool(&db, &client, &name).await?;
                    println!("⏸  Pool '{}' paused", name);
                }
                PoolCommands::Resume { name } => {
                    pool::resume_pool(&db, &client, &name).await?;
                    println!("▶  Pool '{}' resumed", name);
                }
                PoolCommands::Drain { name } => {
                    pool::drain_pool(&db, &docker_ctl, &client, &name).await?;
                    println!("✅ Pool '{}' drained", name);
                }
                PoolCommands::Delete { name } => {
                    pool::delete_pool(&db, &docker_ctl, &client, &name).await?;
                    println!("✅ Pool '{}' deleted", name);
                }
                PoolCommands::RotateToken { name } => {
                    let new_token =
                        pool::rotate_pool_token(&db, &docker_ctl, &client, &name).await?;
                    println!(
                        "🔑 Pool '{}' token rotated: {}...{}",
                        name,
                        &new_token[..8],
                        &new_token[new_token.len() - 4..]
                    );
                }
            }
        }

        // ---- Job ---------------------------------------------------------
        Commands::Job(subcmd) => {
            let (client, _) = load_client()?;

            match subcmd {
                JobCommands::Clear => {
                    let db = state::Db::open().await?;
                    db.clear_history().await?;
                    println!("🗑 All jobs and pipelines cleared from local DB.");
                }
                JobCommands::List { project_id, status } => {
                    let scopes: Vec<&str> = status.split(',').collect();
                    let jobs = client.list_jobs(project_id, &scopes).await?;
                    println!(
                        "{:<8} {:<20} {:<12} {:<10}",
                        "ID", "NAME", "STATUS", "STAGE"
                    );
                    for j in &jobs {
                        println!(
                            "{:<8} {:<20} {:<12} {:<10}",
                            j.id, j.name, j.status, j.stage
                        );
                    }
                }
                JobCommands::Trace { project_id, job_id } => {
                    let trace = client.job_trace(project_id, job_id).await?;
                    logs::print_trace(project_id, job_id, &trace);
                }
                JobCommands::Play { project_id, job_id } => {
                    client.play_job(project_id, job_id).await?;
                    println!("▶ Job {} triggered", job_id);
                }
                JobCommands::Cancel { project_id, job_id } => {
                    client.cancel_job(project_id, job_id).await?;
                    println!("⏹ Job {} cancelled", job_id);
                }
                JobCommands::Retry { project_id, job_id } => {
                    client.retry_job(project_id, job_id).await?;
                    println!("🔄 Job {} retried", job_id);
                }
                JobCommands::Explain { project_id, job_id } => {
                    let db = state::Db::open().await?;
                    if let Some(capsule) = db.latest_evidence_for_job(project_id, job_id).await? {
                        let retry_record = db.latest_retry_decision(project_id, job_id).await?;
                        println!("Job:          {}", capsule.job_id);
                        println!("Pipeline:     {}", capsule.pipeline_id.unwrap_or_default());
                        println!("Stage:        {}", capsule.stage);
                        println!("Ref:          {}", capsule.ref_name);
                        println!("Commit:       {}", capsule.commit_sha);
                        println!("Failure kind: {}", capsule.failure_kind);
                        println!("Classified:   {:?}", capsule.classify());
                        println!("Retry advice: {:?}", capsule.recommended_retry());
                        println!("Summary:      {}", capsule.summary);
                        if let Some(record) = retry_record {
                            println!("Last retry:   {} ({})", record.decision, record.reason);
                        }
                        println!("\nLog snippet:\n{}", capsule.log_snippet);
                    } else {
                        println!("No structured evidence capsule found for job {}", job_id);
                    }
                }
            }
        }

        // ---- Pipeline ----------------------------------------------------
        Commands::Pipeline(subcmd) => {
            let (client, _) = load_client()?;
            match subcmd {
                PipelineCommands::Explain {
                    project_id,
                    pipeline_id,
                    json,
                } => {
                    let report =
                        release::build_pipeline_explain_report(&client, project_id, pipeline_id)
                            .await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        print!("{}", release::render_pipeline_explain_text(&report));
                    }
                }
                PipelineCommands::Doctor {
                    project_id,
                    pipeline_id,
                    json,
                } => {
                    let report =
                        release::build_pipeline_doctor_report(&client, project_id, pipeline_id)
                            .await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        print!("{}", release::render_pipeline_doctor_text(&report));
                    }
                }
                PipelineCommands::Jobs {
                    project_id,
                    pipeline_id,
                    ingest,
                    json,
                } => {
                    let db = state::Db::open().await?;
                    let runs = fetch_ci_job_runs(&client, project_id, pipeline_id).await?;
                    if ingest {
                        db.upsert_ci_job_runs(&runs).await?;
                    }
                    if json {
                        println!("{}", serde_json::to_string_pretty(&runs)?);
                    } else {
                        println!("Pipeline {} — {} jobs", pipeline_id, runs.len());
                        for run in &runs {
                            let dur = run
                                .duration_secs
                                .map(|value| format!("{value:.1}s"))
                                .unwrap_or_else(|| "-".to_string());
                            let queue = run
                                .queued_duration_secs
                                .map(|value| format!("{value:.1}s"))
                                .unwrap_or_else(|| "-".to_string());
                            println!(
                                "  {:<34} {:<10} stage={:<14} run={:>8} queue={:>8} start={} finish={}",
                                run.job_name,
                                run.status,
                                run.stage,
                                dur,
                                queue,
                                run.started_at.as_deref().unwrap_or("-"),
                                run.finished_at.as_deref().unwrap_or("-")
                            );
                        }
                    }
                }
                PipelineCommands::Ingest {
                    project_id,
                    pipeline_id,
                } => {
                    let db = state::Db::open().await?;
                    let runs = fetch_ci_job_runs(&client, project_id, pipeline_id).await?;
                    db.upsert_ci_job_runs(&runs).await?;
                    println!(
                        "ingested {} job runs for pipeline {}",
                        runs.len(),
                        pipeline_id
                    );
                }
                PipelineCommands::Cancel {
                    project_id,
                    pipeline_id,
                } => {
                    client.cancel_pipeline(project_id, pipeline_id).await?;
                    println!("cancelled pipeline {}", pipeline_id);
                }
                PipelineCommands::Bottlenecks {
                    project_id,
                    ref_name,
                    limit,
                    json,
                } => {
                    let db = state::Db::open().await?;
                    let rows = db
                        .ci_job_bottlenecks(project_id, ref_name.as_deref(), limit)
                        .await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&rows)?);
                    } else {
                        println!("CI bottlenecks — project {}", project_id);
                        for row in &rows {
                            println!(
                                "  {:<34} avg={:>7.1}s latest={:>7} max={:>7} runs={:<4} stage={} pool={}",
                                row.job_name,
                                row.avg_duration_secs,
                                row.latest_duration_secs
                                    .map(|value| format!("{value:.1}s"))
                                    .unwrap_or_else(|| "-".to_string()),
                                row.max_duration_secs
                                    .map(|value| format!("{value:.1}s"))
                                    .unwrap_or_else(|| "-".to_string()),
                                row.runs,
                                row.stage,
                                row.runner_pool.as_deref().unwrap_or("-")
                            );
                        }
                    }
                }
            }
        }

        // ---- Cache -------------------------------------------------------
        Commands::Cache(subcmd) => {
            let db = state::Db::open().await?;
            let sc = cache::SmartCache::new(db);
            match subcmd {
                CacheCommands::Enable => {
                    sc.enable().await?;
                }
                CacheCommands::Doctor => {
                    sc.doctor().await?;
                }
                CacheCommands::Status { json } => {
                    sc.status_with_options(json).await?;
                }
                CacheCommands::Gc {
                    dry_run,
                    json,
                    keep_active_managers,
                    older_than,
                    max_cache_gb,
                } => {
                    sc.gc_with_options(cache::GcOptions {
                        dry_run,
                        json,
                        keep_active_managers,
                        older_than,
                        max_cache_gb,
                        quiet: false,
                    })
                    .await?;
                }
            }
        }

        // ---- Local ------------------------------------------------------
        Commands::Local(subcmd) => match subcmd {
            LocalCommands::Cargo { repo, cargo_args } => {
                local::run_cargo(repo, cargo_args).await?;
            }
            LocalCommands::CargoEnv { repo, json } => {
                let layout = local::cargo_env(repo)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&layout)?);
                } else {
                    let exports = cargo_cache::shell_exports(&layout);
                    if !exports.is_empty() {
                        println!("{}", exports.join("\n"));
                    }
                }
            }
        },

        // ---- Logs --------------------------------------------------------
        Commands::Logs { manager_id, lines } => {
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;
            let log_lines = logs::tail_manager(&db, &docker_ctl, &manager_id, lines).await?;
            logs::print_manager_logs(&manager_id, &log_lines);
        }

        // ---- Agent -------------------------------------------------------
        Commands::Agent(subcmd) => {
            let (client, _) = load_client()?;

            match subcmd {
                AgentCommands::Spawn { project_id, task } => {
                    let agent_task = agent::spawn_agent(&client, project_id, &task).await?;
                    println!("🤖 Agent spawned!");
                    println!("   Project:  {}", agent_task.project_id);
                    println!("   Branch:   {}", agent_task.branch_name);
                    println!("   Issue:    #{}", agent_task.issue_iid.unwrap_or(0));
                    println!("   Task:     {}", agent_task.task_description);
                }
                AgentCommands::List { project_id } => {
                    let agents = agent::list_agents(&client, project_id).await?;
                    if agents.is_empty() {
                        println!("No active agents.");
                    } else {
                        for a in &agents {
                            println!("  #{:<5} [{}] {}", a.iid, a.labels.join(", "), a.title);
                        }
                    }
                }
                AgentCommands::Merge {
                    project_id,
                    mr_iid,
                    trust_tier,
                } => {
                    let trust_tier = trust_tier
                        .parse::<decision::TrustTier>()
                        .unwrap_or(decision::TrustTier::Trusted);
                    let evaluation =
                        agent::merge_agent_mr(&client, project_id, mr_iid, trust_tier).await?;
                    println!("Risk gate: {:?}", evaluation.decision);
                    println!("Reason:    {}", evaluation.reason);
                }
            }
        }

        // ---- Test --------------------------------------------------------
        Commands::Test(subcmd) => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;

            match subcmd {
                TestCommands::Run {
                    command,
                    project_id,
                    image,
                    tags,
                    timeout,
                    force,
                } => {
                    let commit_sha = std::process::Command::new("git")
                        .args(["log", "-1", "--format=%H"])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_else(|_| "latest".to_string());

                    let tag_list = tags.map(|tags| {
                        tags.split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    });

                    let opts = test_runner::TestRunOpts {
                        project_id,
                        test_command: command,
                        job_name: None,
                        image,
                        tags: tag_list,
                        timeout_secs: timeout,
                        force,
                        commit_sha,
                    };
                    println!("━━━ jeryu test run ━━━\n");
                    println!("  Project ID: {}", opts.project_id);
                    println!("  Command:    {}", opts.test_command);
                    let plan = test_runner::plan_test_run(&opts);
                    println!("  Inferred Routing:");
                    println!("    Risk Class: {}", plan.risk_class);
                    println!("    Tags:       {:?}", plan.tags);
                    for reason in &plan.rationale {
                        println!("      - {}", reason);
                    }
                    println!("\nExecuting pipeline...");

                    let result = test_runner::run_test(&db, &client, &opts).await?;
                    println!(
                        "\nResult: {}",
                        if result.passed {
                            "✅ Passed"
                        } else {
                            "❌ Failed"
                        }
                    );
                    if let Some(dur) = result.duration_secs {
                        println!("Duration: {:.1}s", dur);
                    }
                    if !result.trace_tail.is_empty() {
                        println!("\nTrace tail:\n{}", result.trace_tail);
                    }
                }
                TestCommands::Plan {
                    command,
                    project_id,
                    image,
                    tags,
                    timeout,
                } => {
                    let tag_list = tags.map(|tags| {
                        tags.split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    });

                    let opts = test_runner::TestRunOpts {
                        project_id,
                        test_command: command,
                        job_name: None,
                        image,
                        tags: tag_list,
                        timeout_secs: timeout,
                        force: false,
                        commit_sha: String::new(),
                    };
                    println!("━━━ jeryu test plan ━━━\n");
                    let plan = test_runner::plan_test_run(&opts);
                    println!("  Command:      {}", plan.command);
                    println!("  Risk Class:   {}", plan.risk_class);
                    println!("  Tags:         {:?}", plan.tags);
                    println!("  Timeout:      {}s", plan.timeout_secs);
                    println!("  Rationale:");
                    for reason in &plan.rationale {
                        println!("    - {}", reason);
                    }
                }
                TestCommands::Batch {
                    commands,
                    project_id,
                    image,
                    tags,
                    timeout,
                    max_parallel,
                    force,
                } => {
                    let commit_sha = std::process::Command::new("git")
                        .args(["log", "-1", "--format=%H"])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_else(|_| "latest".to_string());

                    let tag_list = tags.map(|tags| {
                        tags.split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    });

                    let opts = test_runner::TestBatchOpts {
                        project_id,
                        test_commands: commands.clone(),
                        job_name_prefix: Some("batch-test".to_string()),
                        image,
                        tags: tag_list,
                        timeout_secs: timeout,
                        max_parallel,
                        force,
                        commit_sha,
                    };
                    println!("🧪 Starting batched test run...");
                    println!("   Commands:  {}", opts.test_commands.len());
                    println!("   Image:     {}", opts.image);
                    println!(
                        "   Tags:      {}",
                        opts.tags
                            .as_ref()
                            .map(|tags| format!("{:?}", tags))
                            .unwrap_or_else(|| "smart-inferred".to_string())
                    );
                    println!("   Parallel:  {}", opts.max_parallel);
                    println!();
                    let results = test_runner::run_test_batch(&db, &client, &opts).await?;
                    let passed = results.iter().filter(|r| r.passed).count();
                    let failed = results.iter().filter(|r| !r.passed).count();
                    println!("✅ Batch complete: {} passed, {} failed", passed, failed);
                    for r in &results {
                        let icon = if r.passed { "✅" } else { "❌" };
                        println!(
                            "  {} {:<34} {:<10} pipeline={}",
                            icon, r.job_name, r.status, r.pipeline_id
                        );
                    }
                }
                TestCommands::Results {
                    pipeline_id,
                    project_id,
                } => {
                    let results =
                        test_runner::pipeline_results(&client, project_id, pipeline_id).await?;

                    let passed = results.iter().filter(|r| r.passed).count();
                    let failed = results.iter().filter(|r| r.status == "failed").count();
                    let skipped = results.iter().filter(|r| r.status == "skipped").count();
                    let other = results.len() - passed - failed - skipped;

                    println!("Pipeline {} — {} jobs", pipeline_id, results.len());
                    println!(
                        "  ✅ {} passed  ❌ {} failed  ⏭ {} skipped  ⏳ {} other",
                        passed, failed, skipped, other
                    );
                    println!();

                    for r in &results {
                        let icon = match r.status.as_str() {
                            "success" => "✅",
                            "failed" => "❌",
                            "skipped" => "⏭ ",
                            "running" => "🔄",
                            "pending" | "created" => "⏳",
                            _ => "❓",
                        };
                        let dur = r
                            .duration_secs
                            .map(|d| format!("{:.0}s", d))
                            .unwrap_or_default();
                        println!("  {} {:<40} {:>8} {}", icon, r.job_name, r.status, dur);
                    }
                }
                TestCommands::Retry {
                    pipeline_id,
                    job_name,
                    project_id,
                } => {
                    println!(
                        "🔄 Retrying job '{}' in pipeline {}...",
                        job_name, pipeline_id
                    );
                    let result =
                        test_runner::retry_job_by_name(&client, project_id, pipeline_id, &job_name)
                            .await?;

                    if result.passed {
                        println!("✅ Job '{}' passed on retry!", job_name);
                    } else {
                        println!("❌ Job '{}' still failing: {}", job_name, result.status);
                    }
                }
                TestCommands::Failed {
                    pipeline_id,
                    project_id,
                } => {
                    let results =
                        test_runner::pipeline_results(&client, project_id, pipeline_id).await?;

                    let failed: Vec<_> = results
                        .into_iter()
                        .filter(|r| r.status == "failed")
                        .collect();

                    if failed.is_empty() {
                        println!("✅ No failed jobs in pipeline {}!", pipeline_id);
                    } else {
                        println!(
                            "❌ {} failed job(s) in pipeline {}:\n",
                            failed.len(),
                            pipeline_id
                        );
                        for r in &failed {
                            println!("━━━ {} (id={:?}) ━━━", r.job_name, r.job_id);
                            if !r.trace_tail.is_empty() {
                                let lines: Vec<&str> = r.trace_tail.lines().collect();
                                let start = lines.len().saturating_sub(20);
                                for line in &lines[start..] {
                                    println!("  {}", line);
                                }
                            }
                            println!();
                        }
                    }
                }
                TestCommands::Impact {
                    base,
                    head,
                    repo_root,
                    json,
                } => {
                    let output = tokio::process::Command::new("cargo")
                        .current_dir(&repo_root)
                        .args([
                            "run",
                            "-q",
                            "-p",
                            "veox-testctl",
                            "--",
                            "ci-impact",
                            "--base",
                            &base,
                            "--head",
                            &head,
                            "--json",
                        ])
                        .output()
                        .await?;
                    if !output.status.success() {
                        anyhow::bail!(
                            "ci-impact failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    if json {
                        print!("{}", String::from_utf8_lossy(&output.stdout));
                    } else {
                        let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
                        println!("━━━ jeryu test impact ━━━\n");
                        println!("  Base/head:          {base}..{head}");
                        println!(
                            "  Release impacting:  {}",
                            value["release_impacting"].as_bool().unwrap_or(true)
                        );
                        println!(
                            "  Full build:         {}",
                            value["full_build_required"].as_bool().unwrap_or(true)
                        );
                        let jobs = value["jobs"]
                            .as_array()
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(|item| item.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();
                        println!("  Jobs:               {jobs}");
                        let rules = value["matched_rules"]
                            .as_array()
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(|item| item.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();
                        println!("  Matched rules:      {rules}");
                    }
                }
                TestCommands::Select {
                    base,
                    head,
                    repo_root,
                    explain,
                    json,
                    emit_gitlab,
                    emit_plan,
                    emit_receipt,
                } => {
                    let cwd = repo_root.unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });

                    // Get changed files via git diff
                    let diff_output = std::process::Command::new("git")
                        .current_dir(&cwd)
                        .args(["diff", "--name-only", &base, &head])
                        .output()?;

                    if !diff_output.status.success() {
                        anyhow::bail!(
                            "git diff failed: {}",
                            String::from_utf8_lossy(&diff_output.stderr)
                        );
                    }

                    let changed_paths: Vec<String> = String::from_utf8_lossy(&diff_output.stdout)
                        .lines()
                        .filter(|line| !line.is_empty())
                        .map(|line| line.to_string())
                        .collect();

                    // Run the VTI planner
                    let plan = test_intel::planner::plan_tests(&changed_paths);
                    let receipt = plan.receipt(Some(&base), Some(&head));

                    // Output
                    if json {
                        let json_value = test_intel::explain::explain_json(&plan);
                        println!("{}", serde_json::to_string_pretty(&json_value)?);
                    } else if explain {
                        print!("{}", test_intel::explain::explain(&plan));
                    } else {
                        println!("━━━ jeryu test select ━━━\n");
                        println!("  Base:       {}", base);
                        println!("  Head:       {}", head);
                        println!("  Changed:    {} files", changed_paths.len());
                        println!("  Mode:       {:?}", plan.mode);
                        println!("  Confidence: {:.2}", plan.confidence);
                        println!("  Receipt:    {}", receipt.receipt_id);
                        println!("  Selected:   {} test commands", plan.selected_tests.len());
                        println!("  Skipped:    {} subsystems", plan.skipped_subsystems.len());
                        if let Some(reason) = &plan.fallback_reason {
                            println!("  Fallback:   {}", reason);
                        }
                        println!();
                        for test in &plan.selected_tests {
                            println!("  ✓ [{}] {}", test.subsystem, test.command);
                        }
                    }

                    // Emit artifacts
                    if let Some(gitlab_path) = emit_gitlab {
                        let yaml = test_intel::ci_gen::emit_gitlab_child_yaml(&plan);
                        if let Some(parent) = gitlab_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&gitlab_path, &yaml)?;
                        eprintln!("Wrote GitLab child pipeline to {}", gitlab_path.display());
                    }

                    if let Some(plan_path) = emit_plan {
                        let json_value = test_intel::explain::explain_json(&plan);
                        if let Some(parent) = plan_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&plan_path, serde_json::to_string_pretty(&json_value)?)?;
                        eprintln!("Wrote test plan to {}", plan_path.display());
                    }

                    if let Some(receipt_path) = emit_receipt {
                        if let Some(parent) = receipt_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?)?;
                        eprintln!("Wrote VTI receipt to {}", receipt_path.display());
                    }
                }
                TestCommands::ExplainPlan { plan_path } => {
                    let contents = std::fs::read_to_string(&plan_path)?;
                    let plan: test_intel::planner::TestPlan = serde_json::from_str(&contents)?;
                    print!("{}", test_intel::explain::explain(&plan));
                }
                TestCommands::SelectExternal {
                    base,
                    head,
                    workspace,
                    explain,
                    json,
                    emit_gitlab,
                    emit_plan,
                    emit_skipped,
                } => {
                    let testmap_path = workspace.join(".jeryu/testmap.toml");
                    if !testmap_path.exists() {
                        anyhow::bail!("no .jeryu/testmap.toml found at {}", testmap_path.display());
                    }

                    let map = test_intel::testmap::load_testmap(&testmap_path)
                        .map_err(|e| anyhow::anyhow!(e))?;

                    // Get changed files via git diff
                    let diff_output = std::process::Command::new("git")
                        .current_dir(&workspace)
                        .args(["diff", "--name-only", &base, &head])
                        .output()?;

                    if !diff_output.status.success() {
                        anyhow::bail!(
                            "git diff failed: {}",
                            String::from_utf8_lossy(&diff_output.stderr)
                        );
                    }

                    let changed_paths: Vec<String> = String::from_utf8_lossy(&diff_output.stdout)
                        .lines()
                        .filter(|line| !line.is_empty())
                        .map(|line| line.to_string())
                        .collect();

                    let plan = test_intel::testmap::plan_from_testmap(&map, &changed_paths);

                    // Output
                    if json {
                        let json_value = test_intel::testmap::explain_external_json(&plan);
                        println!("{}", serde_json::to_string_pretty(&json_value)?);
                    } else if explain {
                        print!("{}", test_intel::testmap::explain_external_plan(&plan));
                    } else {
                        println!("━━━ jeryu test select-external ━━━\n");
                        println!("  Workspace: {}", workspace.display());
                        println!("  Base:      {}", base);
                        println!("  Head:      {}", head);
                        println!("  Changed:   {} files", changed_paths.len());
                        println!("  Mode:      {:?}", plan.mode);
                        println!("  Confidence:{:.2}", plan.confidence);
                        println!("  Selected:  {} CI jobs", plan.selected_jobs.len());
                        println!("  Skipped:   {} CI jobs", plan.skipped_jobs.len());
                        if let Some(reason) = &plan.fallback_reason {
                            println!("  Fallback:  {}", reason);
                        }
                        println!();
                        for job in &plan.selected_jobs {
                            println!("  ✓ {}", job);
                        }
                    }

                    // Emit artifacts
                    if let Some(gitlab_path) = emit_gitlab {
                        let yaml =
                            test_intel::testmap::emit_external_gitlab_yaml(&plan, Some(&workspace));
                        if let Some(parent) = gitlab_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&gitlab_path, &yaml)?;
                        eprintln!("Wrote GitLab child pipeline to {}", gitlab_path.display());
                    }

                    if let Some(plan_path) = emit_plan {
                        let json_value = test_intel::testmap::explain_external_json(&plan);
                        if let Some(parent) = plan_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&plan_path, serde_json::to_string_pretty(&json_value)?)?;
                        eprintln!("Wrote test plan to {}", plan_path.display());
                    }

                    if let Some(skipped_path) = emit_skipped {
                        let json_value = test_intel::testmap::explain_external_skipped_json(&plan);
                        if let Some(parent) = skipped_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&skipped_path, serde_json::to_string_pretty(&json_value)?)?;
                        eprintln!("Wrote VTI skipped metadata to {}", skipped_path.display());
                    }
                }
                TestCommands::Audit {
                    changed,
                    failed,
                    all_tests,
                    sha,
                    json,
                    workspace,
                } => {
                    let changed_paths: Vec<String> = changed
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let failed_tests: Vec<String> = failed
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let all_test_list: Vec<String> = all_tests
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    let mut external_map = None;
                    if let Some(workspace_path) = workspace {
                        let testmap_path = workspace_path.join(".jeryu/testmap.toml");
                        if testmap_path.exists() {
                            external_map = match test_intel::testmap::load_testmap(&testmap_path) {
                                Ok(m) => Some(m),
                                Err(e) => {
                                    eprintln!("Warning: failed to load testmap: {}", e);
                                    None
                                }
                            };
                        }
                    }

                    let report = test_intel::nightly::audit_selector(
                        &changed_paths,
                        &failed_tests,
                        &all_test_list,
                        &sha,
                        external_map.as_ref(),
                    );

                    if json {
                        let json_value = test_intel::nightly::explain_audit_json(&report);
                        println!("{}", serde_json::to_string_pretty(&json_value)?);
                    } else {
                        print!("{}", test_intel::nightly::explain_audit(&report));
                    }

                    // Persist misses to DB
                    // (plan_id=None for CLI audits — no linked test_plans record)
                    for miss in &report.misses {
                        if let Err(e) = db
                            .record_selector_miss(
                                None,
                                &miss.missed_test,
                                &miss.failed_sha,
                                &miss.detected_by,
                            )
                            .await
                        {
                            eprintln!("Warning: failed to persist selector miss: {}", e);
                        }
                    }
                }
                TestCommands::Learn {
                    changed,
                    failed,
                    all_tests,
                    sha,
                    json,
                    workspace,
                } => {
                    let changed_paths: Vec<String> = changed
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let failed_tests: Vec<String> = failed
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    let all_test_list: Vec<String> = all_tests
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    let mut external_map = None;
                    if let Some(workspace_path) = workspace {
                        let testmap_path = workspace_path.join(".jeryu/testmap.toml");
                        if testmap_path.exists() {
                            external_map = match test_intel::testmap::load_testmap(&testmap_path) {
                                Ok(m) => Some(m),
                                Err(e) => {
                                    eprintln!("Warning: failed to load testmap: {}", e);
                                    None
                                }
                            };
                        }
                    }

                    let report = test_intel::nightly::audit_selector(
                        &changed_paths,
                        &failed_tests,
                        &all_test_list,
                        &sha,
                        external_map.as_ref(),
                    );
                    let result = test_intel::nightly::learn_from_audit(&report);

                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "processed": result.processed,
                                "new_misses": result.new_misses,
                                "flagged_subsystems": result.flagged_subsystems,
                                "suggestions": result.suggestions,
                            }))?
                        );
                    } else {
                        println!("━━━ VTI Learn ━━━\n");
                        println!("  Processed: {} tests", result.processed);
                        println!("  New misses: {}", result.new_misses);
                        if !result.flagged_subsystems.is_empty() {
                            println!("  Flagged:   {}", result.flagged_subsystems.join(", "));
                        }
                        println!();
                        for suggestion in &result.suggestions {
                            println!("  {}", suggestion);
                        }
                    }
                }
                TestCommands::CacheStatus { base, head, json } => {
                    // 1. Get changed paths
                    let diff_output = std::process::Command::new("git")
                        .args(["diff", "--name-only", &base, &head])
                        .output()?;
                    let changed_paths: Vec<String> = String::from_utf8_lossy(&diff_output.stdout)
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| l.to_string())
                        .collect();

                    // 2. Compute test plan
                    let plan = test_intel::planner::plan_tests(&changed_paths);

                    // 3. Compute source hashes for changed files
                    let mut source_hashes: Vec<(String, String)> = Vec::new();
                    for path in &changed_paths {
                        let hash_output = std::process::Command::new("git")
                            .args(["hash-object", path])
                            .output();
                        let hash = match hash_output {
                            Ok(out) if out.status.success() => {
                                String::from_utf8_lossy(&out.stdout).trim().to_string()
                            }
                            _ => "unknown".to_string(),
                        };
                        source_hashes.push((path.clone(), hash));
                    }

                    // 4. Get Cargo.lock hash and rustc version
                    let lock_hash = std::process::Command::new("git")
                        .args(["hash-object", "Cargo.lock"])
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_else(|| "no-lock".to_string());

                    let rustc_ver = std::process::Command::new("rustc")
                        .args(["--version"])
                        .output()
                        .ok()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    // 5. Compute cache keys for each selected test
                    let source_refs: Vec<(&str, &str)> = source_hashes
                        .iter()
                        .map(|(p, h)| (p.as_str(), h.as_str()))
                        .collect();

                    let tests_with_keys: Vec<(String, test_intel::cache::TestCacheKey)> = plan
                        .selected_tests
                        .iter()
                        .map(|t| {
                            let key = test_intel::cache::compute_cache_key(
                                &t.command,
                                &source_refs,
                                &lock_hash,
                                &rustc_ver,
                                1, // epoch
                            );
                            (t.command.clone(), key)
                        })
                        .collect();

                    // 6. Check against cache (empty for now — no persisted verdicts yet)
                    let result = test_intel::cache::check_cache(&tests_with_keys, &[]);

                    if json {
                        let json_value = test_intel::cache::explain_cache_json(&result);
                        println!("{}", serde_json::to_string_pretty(&json_value)?);
                    } else {
                        println!("━━━ jeryu test cache-status ━━━\n");
                        println!("  Base: {}", base);
                        println!("  Head: {}", head);
                        println!("  Changed: {} files", changed_paths.len());
                        println!(
                            "  Plan: {:?}, {} commands",
                            plan.mode,
                            plan.selected_tests.len()
                        );
                        println!();
                        print!("{}", test_intel::cache::explain_cache_lookup(&result));
                    }
                }
            }
        }

        // ---- Shadow ------------------------------------------------------
        Commands::Shadow(subcmd) => match subcmd {
            ShadowCommands::Add {
                source,
                project_id,
                branch,
                enable,
            } => {
                let db = state::Db::open().await?;
                let path = std::fs::canonicalize(&source)?;
                let source_dir = path.to_string_lossy().to_string();

                let config = state::ShadowSyncConfig {
                    source_dir: source_dir.clone(),
                    enabled: enable,
                    target_project_id: project_id,
                    target_branch: branch,
                    last_seen_head_sha: None,
                    last_pushed_sha: None,
                    last_attempt_at: None,
                    last_success_at: None,
                    status: if enable {
                        "idle".to_string()
                    } else {
                        "disabled".to_string()
                    },
                    error_msg: None,
                    consecutive_failures: 0,
                    upstream_status: "unconfigured".to_string(),
                    upstream_last_pushed_sha: None,
                    upstream_error_msg: None,
                };
                db.upsert_shadow_sync_config(&config).await?;
                println!("✅ Shadow sync configured for {}", source_dir);
            }
            ShadowCommands::Enable { source } => {
                let db = state::Db::open().await?;
                let path = std::fs::canonicalize(&source)?;
                let source_dir = path.to_string_lossy().to_string();
                db.set_shadow_sync_enabled(&source_dir, true).await?;
                println!("✅ Shadow sync enabled for {}", source_dir);
            }
            ShadowCommands::Disable { source } => {
                let db = state::Db::open().await?;
                let path = std::fs::canonicalize(&source)?;
                let source_dir = path.to_string_lossy().to_string();
                db.set_shadow_sync_enabled(&source_dir, false).await?;
                println!("⏸ Shadow sync disabled for {}", source_dir);
            }
            ShadowCommands::Remove { source } => {
                let db = state::Db::open().await?;
                let path = std::fs::canonicalize(&source)?;
                let source_dir = path.to_string_lossy().to_string();
                db.delete_shadow_sync_config(&source_dir).await?;
                println!("🗑 Shadow sync removed for {}", source_dir);
            }
            ShadowCommands::SyncNow { source } => {
                let db = state::Db::open().await?;
                let path = std::fs::canonicalize(&source)?;
                let source_dir = path.to_string_lossy().to_string();
                db.request_shadow_sync(&source_dir).await?;
                println!(
                    "✅ Sync requested for {}. The shadow worker will pick it up within ~2s.",
                    source_dir
                );
            }
            ShadowCommands::Status { source } => {
                let db = state::Db::open().await?;
                if let Some(src) = source {
                    let path = std::fs::canonicalize(&src)?;
                    let source_dir = path.to_string_lossy().to_string();
                    if let Some(c) = db.get_shadow_sync_config(&source_dir).await? {
                        println!(
                            "Status for {}: {} ({})",
                            source_dir,
                            c.status,
                            if c.enabled { "enabled" } else { "disabled" }
                        );
                    } else {
                        println!("No config found for {}", source_dir);
                    }
                } else {
                    let configs = db.list_shadow_sync_configs().await?;
                    if configs.is_empty() {
                        println!("No shadow sync configs.");
                    } else {
                        for c in configs {
                            println!(
                                "- {} -> Project {} branch {} | status: {} {}",
                                c.source_dir,
                                c.target_project_id,
                                c.target_branch,
                                c.status,
                                if c.enabled { "[ENABLED]" } else { "[DISABLED]" }
                            );
                        }
                    }
                }
            }
        },

        // ---- Shadow Remote ----------------------------------------------
        Commands::ShadowRemote(subcmd) => match subcmd {
            ShadowRemoteCommands::Status { repo, name } => {
                let status = shadow::status(repo.as_deref(), &name)?;
                println!("━━━ jeryu shadow status ━━━\n");
                println!("  Repo:         {}", status.repo_root.display());
                println!(
                    "  Head branch:  {}",
                    status.head_branch.as_deref().unwrap_or("(detached)")
                );
                println!(
                    "  Target remote {}: {}",
                    status.target_remote,
                    if status.target_exists {
                        "present"
                    } else {
                        "missing"
                    }
                );
                println!("\n  Remotes:");
                for remote in &status.remotes {
                    println!(
                        "    {:<12} fetch={} push={}",
                        remote.name,
                        remote.fetch_url.as_deref().unwrap_or("(none)"),
                        remote.push_url
                    );
                }
                println!();
            }
            ShadowRemoteCommands::Ensure { repo, name, url } => {
                shadow::ensure_remote(repo.as_deref(), &name, &url)?;
                println!("✅ Remote '{}' now points to {}", name, url);
            }
            ShadowRemoteCommands::Push {
                repo,
                name,
                branch,
                mirror,
            } => {
                shadow::push_remote(repo.as_deref(), &name, branch.as_deref(), mirror)?;
                if mirror {
                    println!("✅ Mirrored repository to remote '{}'", name);
                } else {
                    println!(
                        "✅ Pushed HEAD to remote '{}'{}",
                        name,
                        branch
                            .as_deref()
                            .map(|branch| format!(" as {branch}"))
                            .unwrap_or_default()
                    );
                }
            }
        },

        // ---- Release ----------------------------------------------------
        Commands::Release(subcmd) => match subcmd {
            ReleaseCommands::Status {
                project_id,
                ref_name,
                sha,
                limit,
                json,
            } => {
                let db = state::Db::open().await?;
                let report = release::build_release_status_report(
                    &db,
                    release::ReleaseStatusQuery {
                        project_id: Some(project_id),
                        ref_name: Some(ref_name),
                        sha,
                        limit,
                    },
                )
                .await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print!("{}", release::render_release_status_text(&report));
                }
            }
            ReleaseCommands::Watch {
                project_id,
                ref_name,
                sha,
                limit,
                interval_secs,
                json,
            } => {
                let db = state::Db::open().await?;
                release::watch_release_status(
                    &db,
                    release::ReleaseStatusQuery {
                        project_id: Some(project_id),
                        ref_name: Some(ref_name),
                        sha,
                        limit,
                    },
                    json,
                    interval_secs,
                )
                .await?;
            }
            ReleaseCommands::Reconcile {
                project_id,
                ref_name,
                json,
            } => {
                let (client, _) = load_client()?;
                let db = state::Db::open().await?;
                let report =
                    release::reconcile_release_for_ref(&db, &client, project_id, &ref_name).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print!("{}", release::render_release_status_text(&report));
                }
            }
            ReleaseCommands::PromoteProd {
                project_id,
                ref_name,
                version,
            } => {
                let (client, _) = load_client()?;
                let db = state::Db::open().await?;
                let pipeline_id = release::trigger_production_promotion(
                    &db, &client, project_id, &ref_name, version,
                )
                .await?;
                println!("Triggered production-promotion pipeline {pipeline_id}");
            }
            ReleaseCommands::Preflight { ssh_host, json } => {
                let report = release::release_preflight(ssh_host.as_deref()).await;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    let status = if report.ok { "PASS" } else { "FAIL" };
                    println!("Preflight: {status}");
                    for (k, v) in &report.checks {
                        println!("  {k}: {v}");
                    }
                    if !report.blockers.is_empty() {
                        println!("\nBlockers:");
                        for b in &report.blockers {
                            println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
                        }
                    }
                }
                if !report.ok {
                    std::process::exit(1);
                }
            }
            ReleaseCommands::Doctor {
                version,
                preflight,
                json,
            } => {
                let db = state::Db::open().await?;
                let ver = if let Some(v) = version {
                    v
                } else {
                    // Use latest known version from release status
                    let report = release::build_release_status_report(
                        &db,
                        release::ReleaseStatusQuery {
                            project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                            ref_name: Some("main".into()),
                            sha: None,
                            limit: 1,
                        },
                    )
                    .await?;
                    report
                        .latest
                        .as_ref()
                        .map(|v| v.attempt.version.clone())
                        .ok_or_else(|| anyhow::anyhow!("no known release version; use --version"))?
                };
                let report = release::release_doctor(&ver, preflight).await;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    let status = if report.blockers.is_empty() {
                        "OK"
                    } else {
                        "BLOCKED"
                    };
                    println!("Doctor [{status}]: {}", report.version);
                    println!("  next_action: {}", report.next_action);
                    println!("  canary_complete: {}", report.canary_complete);
                    println!("  prod_complete: {}", report.prod_complete);
                    println!("  safe_to_reconcile: {}", report.safe_to_reconcile);
                    if !report.preflight.is_empty() {
                        println!("\nPreflight:");
                        for (k, v) in &report.preflight {
                            println!("  {k}: {v}");
                        }
                    }
                    println!("\nGates:");
                    for (k, v) in &report.gates {
                        println!("  {k}: {}", if *v { "present" } else { "MISSING" });
                    }
                    if !report.blockers.is_empty() {
                        println!("\nBlockers:");
                        for b in &report.blockers {
                            println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
                        }
                    }
                }
            }
        },

        // ---- Secrets ----------------------------------------------------
        Commands::Secrets(subcmd) => {
            let db = state::Db::open().await?;
            match subcmd {
                SecretsCommands::Init => {
                    let report = secrets::run_secrets_init(Some(&db)).await?;
                    println!(
                        "Vault initialized at {} (mount={}, prefix={})",
                        report.addr, report.mount, report.prefix
                    );
                }
                SecretsCommands::Status { json } => {
                    let report = secrets::vault_status(Some(&db)).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        println!("━━━ jeryu secrets status ━━━");
                        println!("  Vault:       {}", report.addr);
                        println!("  Initialized: {}", report.initialized);
                        println!("  Sealed:      {}", report.sealed);
                        println!("  Healthy:     {}", report.healthy);
                        println!("  Token:       {}", report.token_present);
                        println!("  Mount:       {}", report.mount);
                        println!("  Prefix:      {}", report.prefix);
                        println!("  Bootstrap:   {}", report.bootstrap_file);
                        println!("  Env file:    {}", report.env_file);
                        if let Some(secret_set) = db.latest_release_secret_set("dougx").await? {
                            println!("\n  Latest release secret set:");
                            println!("    Version:   {}", secret_set.version);
                            println!("    Target:    {}", secret_set.target);
                            println!("    Status:    {}", secret_set.status);
                            println!("    Runtime:   {}", secret_set.rendered_runtime_env_path);
                            if let Some(report_path) = secret_set.report_path {
                                println!("    Report:    {}", report_path);
                            }
                        }
                    }
                }
                SecretsCommands::Rotate {
                    repo,
                    version,
                    target,
                } => {
                    if repo != "dougx" {
                        anyhow::bail!("only --repo dougx is supported right now");
                    }
                    let target = target.parse::<secrets::SecretTarget>()?;
                    let (repo_root, deploy_env, runtime_env) = secrets::default_release_paths();
                    let outcome = secrets::rotate_release_secrets(
                        &db,
                        &repo_root,
                        &repo,
                        &version,
                        target,
                        &deploy_env,
                        &runtime_env,
                    )
                    .await?;
                    println!("{}", serde_json::to_string_pretty(&outcome)?);
                }
                SecretsCommands::Finalize {
                    repo,
                    version,
                    target,
                } => {
                    if repo != "dougx" {
                        anyhow::bail!("only --repo dougx is supported right now");
                    }
                    let target = target.parse::<secrets::SecretTarget>()?;
                    let (repo_root, deploy_env, runtime_env) = secrets::default_release_paths();
                    let path = secrets::finalize_release_secrets(
                        &db,
                        &repo_root,
                        &repo,
                        &version,
                        target,
                        &deploy_env,
                        &runtime_env,
                    )
                    .await?;
                    println!("Finalized runtime env: {}", path.display());
                }
                SecretsCommands::Report { repo, version } => {
                    if repo != "dougx" {
                        anyhow::bail!("only --repo dougx is supported right now");
                    }
                    let (repo_root, _, _) = secrets::default_release_paths();
                    let path =
                        secrets::build_release_secret_report(&db, &repo_root, &repo, &version)
                            .await?;
                    println!("Release report: {}", path.display());
                }
                SecretsCommands::Recover { repo, version } => {
                    if repo != "dougx" {
                        anyhow::bail!("only --repo dougx is supported right now");
                    }
                    let (repo_root, _, _) = secrets::default_release_paths();
                    secrets::recover_release_secrets(&db, &repo_root, &repo, &version).await?;
                }
            }
        }

        // ---- Progress ---------------------------------------------------
        Commands::Progress {
            project_id,
            ref_name,
            json,
        } => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let report =
                release::build_progress_report(&db, &client, project_id, &ref_name).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_progress_text(&report));
            }
        }

        // ---- Repo --------------------------------------------------------
        Commands::Repo(subcmd) => match subcmd {
            RepoCommands::RenderAgentIndex { check } => {
                agent_surface::render_agent_index(check)?;
            }
            RepoCommands::AuditAgentSurface { json } => {
                agent_surface::audit_agent_surface(json)?;
            }
        },

        // ---- Host --------------------------------------------------------
        Commands::Host(subcmd) => match subcmd {
            HostCommands::StorageAudit => {
                reclaim::run_storage_audit().await?;
            }
            HostCommands::Doctor { json } => {
                let report = cache::SmartCache::new(state::Db::open().await?)
                    .host_doctor_report()
                    .await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    cache::print_host_doctor_report(&report);
                }
                if !report.ok {
                    anyhow::bail!("host doctor found unhealthy CI host state");
                }
            }
            HostCommands::Reclaim { mode, plan, apply } => {
                if mode != "aggressive" {
                    anyhow::bail!(
                        "Only --mode aggressive is currently supported for host reclaim."
                    );
                }
                if !plan && !apply {
                    anyhow::bail!("You must specify either --plan or --apply.");
                }
                reclaim::run_aggressive_reclaim(apply).await?;
            }
        },

        // ---- Exec --------------------------------------------------------
        Commands::Exec(subcmd) => match subcmd {
            ExecCommands::Config => {
                exec::run_config()?;
            }
            ExecCommands::Prepare => {
                exec::run_prepare().await?;
            }
            ExecCommands::Run { script_path, stage } => {
                exec::run_stage(&script_path, &stage).await?;
            }
            ExecCommands::Cleanup => {
                exec::run_cleanup().await?;
            }
        },

        // ---- Server Hooks ------------------------------------------------
        Commands::ServerHook(subcmd) => match subcmd {
            ServerHookCommands::PreReceive => {
                admission::run_pre_receive_hook().await?;
            }
        },

        // ---- Action list -------------------------------------------------
        Commands::Action(subcmd) => match subcmd {
            ActionCommands::List { json } => {
                use jeryu::tui::action_registry::{self, Surface};
                if json {
                    let entries: Vec<serde_json::Value> = action_registry::REGISTRY
                        .iter()
                        .map(|e| {
                            serde_json::json!({
                                "id": e.id,
                                "label": e.label,
                                "key_hint": e.key_hint,
                                "risk_tier": e.risk_tier.label(),
                                "dry_run": e.dry_run,
                                "description": e.description,
                                "surfaces": e.surfaces.iter().map(|s| match s {
                                    Surface::Cli => "cli",
                                    Surface::Tui => "tui",
                                    Surface::Capability => "capability",
                                }).collect::<Vec<_>>(),
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                } else {
                    println!("{:<24} {:<12} {:<10} DESCRIPTION", "ACTION", "RISK", "KEY");
                    println!("{}", "─".repeat(80));
                    for e in action_registry::REGISTRY {
                        println!(
                            "{:<24} {:<12} {:<10} {}",
                            e.id,
                            e.risk_tier.label(),
                            e.key_hint.unwrap_or(""),
                            e.description,
                        );
                    }
                }
            }
        },

        // ---- Capability Server -------------------------------------------
        Commands::Capability(subcmd) => match subcmd {
            CapabilityCommands::Serve { socket_path } => {
                let (client, _) = load_client()?;
                capability::start_capability_server(&socket_path, client).await?;
            }
        },

        // ---- MCP Adapter -------------------------------------------------
        Commands::Mcp(subcmd) => match subcmd {
            McpCommands::Serve => {
                let (client, _) = load_client()?;
                mcp::start_mcp_stdio(client).await?;
            }
            McpCommands::ServeHttp => {
                let (client, _) = load_client()?;
                let bind = settings::get().mcp.bind.clone();
                mcp::start_mcp_http(client, &bind).await?;
            }
            McpCommands::Tools { json } => {
                let manifest = mcp::tool_manifest();
                if json {
                    println!("{}", serde_json::to_string_pretty(&manifest)?);
                } else {
                    for tool in manifest {
                        println!(
                            "{:<28} {:<18} {}",
                            tool["name"].as_str().unwrap_or(""),
                            tool["title"].as_str().unwrap_or(""),
                            tool["description"].as_str().unwrap_or(""),
                        );
                    }
                }
            }
        },

        // ---- Next --------------------------------------------------------
        Commands::Next {
            project_id,
            ref_name,
        } => {
            let db = state::Db::open().await?;

            let pipelines = db
                .list_active_pipelines_for_ref(project_id, &ref_name)
                .await?;
            let release = db.latest_release_attempt(project_id, &ref_name).await?;
            let since = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
            let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);
            let evidence = db.list_evidence_for_ref(project_id, &ref_name, 1).await?;

            println!("━━━ jeryu next — {ref_name} ━━━\n");

            // Priority 1: recent job failures
            if let Some(rec) = evidence.first() {
                println!("  ● PRIORITY: pipeline failure detected");
                println!(
                    "    job={}  stage={}  kind={}  sha={}",
                    rec.job_id,
                    rec.stage,
                    rec.failure_kind,
                    &rec.commit_sha[..rec.commit_sha.len().min(12)]
                );
                println!(
                    "    Action: jeryu job explain --project-id {project_id} --job-id {}",
                    rec.job_id
                );
                println!();
            }

            // Priority 2: active pipelines
            if !pipelines.is_empty() {
                println!("  ● {} active pipeline(s) for {ref_name}", pipelines.len());
                for p in &pipelines {
                    println!(
                        "    pipeline={}  status={}  updated={}",
                        p.pipeline_id, p.status, p.updated_at
                    );
                }
                println!();
            }

            // Priority 3: release gate
            if let Some(rel) = &release {
                let upstream_ok = rel.upstream_status == "success";
                let canary_ok = rel.canary_status == "passed" || rel.canary_status == "skipped";
                if !upstream_ok {
                    println!(
                        "  ● Release blocked: upstream pipeline status={}",
                        rel.upstream_status
                    );
                    println!(
                        "    Action: jeryu release status --project-id {project_id} --ref-name {ref_name}"
                    );
                } else if !canary_ok {
                    println!(
                        "  ⟳ Release in progress: canary_status={}",
                        rel.canary_status
                    );
                    println!(
                        "    Action: jeryu release status --project-id {project_id} --ref-name {ref_name}"
                    );
                } else {
                    println!(
                        "  ✓ Release gate OK (sha={})",
                        &rel.sha[..rel.sha.len().min(12)]
                    );
                }
                println!();
            } else {
                println!("  ○ No release attempt tracked for {ref_name}");
                println!();
            }

            // Priority 4: selector misses
            if miss_count > 0 {
                println!("  ● {miss_count} unrepaired selector miss(es) in last 7 days");
                println!(
                    "    Action: jeryu test audit --changed <files> --failed <tests> --sha HEAD"
                );
                println!();
            }

            if evidence.is_empty() && pipelines.is_empty() && miss_count == 0 {
                println!("  ✓ No active issues detected for {ref_name}.");
            }
        }

        // ---- ExplainBlocker ----------------------------------------------
        Commands::ExplainBlocker {
            entity_type,
            entity_id,
        } => {
            let db = state::Db::open().await?;

            println!("━━━ jeryu explain-blocker {entity_type}:{entity_id} ━━━\n");

            match entity_type.as_str() {
                "job" => {
                    if let Some(cap) = db.latest_evidence_by_job_id(entity_id).await? {
                        println!(
                            "  job={}  stage={}  ref={}",
                            cap.job_id, cap.stage, cap.ref_name
                        );
                        println!(
                            "  commit:       {}",
                            &cap.commit_sha[..cap.commit_sha.len().min(12)]
                        );
                        println!("  failure_kind: {}", cap.failure_kind);
                        println!("  exit_code:    {}", cap.exit_code);
                        println!("  classified:   {:?}", cap.classify());
                        println!("  retry_advice: {:?}", cap.recommended_retry());
                        println!("  summary:      {}", cap.summary);
                        if !cap.repro_script.is_empty() {
                            println!("\n  Repro script:\n    {}", cap.repro_script);
                        }
                        if !cap.log_snippet.is_empty() {
                            println!("\n  Log (last 10 lines):");
                            for line in cap
                                .log_snippet
                                .lines()
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .take(10)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                            {
                                println!("    {}", line);
                            }
                        }
                        if let Some(sup) = &cap.superseded_by_sha {
                            println!(
                                "\n  Note: superseded by commit {}",
                                &sup[..sup.len().min(12)]
                            );
                        }
                    } else {
                        println!("  No failure capsule found for job {entity_id}.");
                        println!("  Try: jeryu job trace --project-id <id> --job-id {entity_id}");
                    }
                }
                "release" => {
                    let attempts = db.recent_release_attempts(None, None, 20).await?;
                    if let Some(rel) = attempts.iter().find(|r| r.id == entity_id) {
                        println!(
                            "  id={}  ref={}  sha={}",
                            rel.id,
                            rel.ref_name,
                            &rel.sha[..rel.sha.len().min(12)]
                        );
                        println!(
                            "  upstream:     {} (pipeline={:?})",
                            rel.upstream_status, rel.upstream_pipeline_id
                        );
                        println!(
                            "  canary:       {} (started={:?})",
                            rel.canary_status, rel.canary_started_at
                        );
                        println!(
                            "  release_pipe: {:?} status={:?}",
                            rel.release_pipeline_id, rel.release_pipeline_status
                        );
                        println!(
                            "  prod_pipe:    {:?} status={:?}",
                            rel.production_pipeline_id, rel.production_pipeline_status
                        );
                        println!();
                        if rel.upstream_status != "success" {
                            println!(
                                "  BLOCKER: upstream pipeline not green (status={})",
                                rel.upstream_status
                            );
                        }
                        if rel.canary_status == "running" {
                            println!("  WAITING: canary still running");
                        } else if rel.canary_status == "failed" {
                            println!("  BLOCKER: canary failed — {:?}", rel.canary_note);
                        }
                        if rel.production_pipeline_status.as_deref() == Some("failed") {
                            println!("  BLOCKER: production pipeline failed");
                        }
                        println!(
                            "\n  Action: jeryu release status --project-id {} --ref-name {}",
                            rel.project_id, rel.ref_name
                        );
                    } else {
                        println!("  No release attempt with id={entity_id} found.");
                    }
                }
                "merge" => {
                    let since = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
                    let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);
                    println!("  MR iid: {entity_id}");
                    println!("  Selector misses (30d): {miss_count}");
                    if miss_count > 0 {
                        println!("  BLOCKER: {miss_count} unrepaired test selector miss(es).");
                        println!(
                            "  Action:  jeryu test audit --changed <files> --failed <tests> --sha HEAD"
                        );
                    } else {
                        println!("  ✓ No selector misses.");
                    }
                    println!("\n  For full pipeline/approval status:");
                    println!("    jeryu pipeline explain --project-id <id> --pipeline-id <id>");
                }
                other => {
                    println!("  Unknown entity type '{other}'. Supported: job | release | merge");
                }
            }
        }
    }

    Ok(())
}
