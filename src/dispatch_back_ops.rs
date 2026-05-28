use super::*;

pub(crate) async fn run_serve() -> Result<()> {
    let (client, webhook_secret) = load_client().await?;
    let db = state::Db::open().await?;
    let docker_ctl = docker::DockerCtl::connect()?;

    docker_ctl.compose_up().await?;

    if let Err(e) = admission::install_global_hook() {
        tracing::warn!("Failed to install global server hook: {}", e);
    }

    cache::SmartCache::new(db.clone()).start().await?;

    let repaired_pools = pool::ensure_default_pool_rows(&db, &client).await?;
    if repaired_pools > 0 {
        tracing::warn!(
            repaired_pools,
            "repaired missing default runner pool rows before reconciliation"
        );
    }

    let normalized_runners = jeryu::runner_policy::enforce_untagged_runners(&client).await?;
    if normalized_runners > 0 {
        tracing::warn!(
            normalized_runners,
            "normalized GitLab runners to untagged policy before reconciliation"
        );
    }

    let pools = db.list_pools().await?;
    for p in &pools {
        if !p.paused {
            pool::scale_pool_to(&db, &docker_ctl, &client, &p.name, p.min_warm as usize).await?;
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

    tokio::signal::ctrl_c().await?;
    println!("\nShutting down engine...");

    Ok(())
}

pub(crate) async fn run_cache(subcmd: CacheCommands) -> Result<()> {
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
    Ok(())
}

pub(crate) async fn run_local(subcmd: LocalCommands) -> Result<()> {
    match subcmd {
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
    }
    Ok(())
}

pub(crate) async fn run_logs(manager_id: String, lines: usize) -> Result<()> {
    let db = state::Db::open().await?;
    let docker_ctl = docker::DockerCtl::connect()?;
    let log_lines = logs::tail_manager(&db, &docker_ctl, &manager_id, lines).await?;
    logs::print_manager_logs(&manager_id, &log_lines);
    Ok(())
}

pub(crate) async fn run_agent(subcmd: AgentCommands) -> Result<()> {
    if let AgentCommands::Submit {
        task,
        issue,
        risk_tier,
        dry_run,
        json,
    } = subcmd
    {
        crate::commands::agent_submit::execute_agent_submit(task, issue, risk_tier, dry_run, json)
            .await?;
        return Ok(());
    }

    let (client, _) = load_client().await?;
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
            let evaluation = agent::merge_agent_mr(&client, project_id, mr_iid, trust_tier).await?;
            println!("Risk gate: {:?}", evaluation.decision);
            println!("Reason:    {}", evaluation.reason);
        }
        AgentCommands::Submit { .. } => unreachable!("handled above"),
    }
    Ok(())
}

pub(crate) async fn run_progress(project_id: i64, ref_name: String, json: bool) -> Result<()> {
    let (client, _) = load_client().await?;
    let db = state::Db::open().await?;
    let report = release::build_progress_report(&db, &client, project_id, &ref_name).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", release::render_progress_text(&report));
    }
    Ok(())
}
