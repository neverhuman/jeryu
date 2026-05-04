//! Owner: CLI Dispatch
//! Proof: `cargo check -p jeryu`
//! Invariants: All logic dispatches to domain modules; no business logic here
//!
//! Wires CLI commands to domain module functions.

use anyhow::Result;

use crate::cli::*;
use jeryu::*;

// ---------------------------------------------------------------------------
// Helpers

/// Load secrets from jeryu.env and build a GitlabClient.
pub fn load_client() -> Result<(gitlab_client::GitlabClient, String)> {
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
        Commands::Ship => {
            let db = state::Db::open().await.ok();
            return crate::commands::git::execute_ship(db.as_ref()).await;
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
        Commands::Test(subcmd) => crate::commands::test::execute_test_commands(subcmd).await?,

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

        // ---- Mirror -----------------------------------------------------
        Commands::Mirror(subcmd) => {
            crate::commands::mirror::execute_mirror_commands(subcmd).await?
        }

        // ---- Settings ---------------------------------------------------
        Commands::Settings(subcmd) => {
            crate::commands::settings::execute_settings_commands(subcmd).await?
        }

        // ---- Release ----------------------------------------------------
        Commands::Release(subcmd) => {
            crate::commands::release::execute_release_commands(subcmd).await?
        }
        // ---- Secrets ----------------------------------------------------
        Commands::Secrets(subcmd) => {
            crate::commands::secrets::execute_secrets_commands(subcmd).await?
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
        Commands::Repo(subcmd) => {
            return crate::commands::repo::execute_repo_commands(subcmd).await;
        }

        // ---- Host --------------------------------------------------------
        Commands::Host(subcmd) => {
            return crate::commands::host::execute_host_commands(subcmd).await;
        }

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

    Ok(0)
}
