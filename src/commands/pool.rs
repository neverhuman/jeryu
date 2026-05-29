use crate::cli::PoolCommands;
use anyhow::Result;
use jeryu::{docker, pool, state};

pub(crate) async fn execute_pool_commands(subcmd: PoolCommands) -> Result<()> {
    let (client, _) = crate::dispatch::load_client().await?;
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
        PoolCommands::Doctor { json } => {
            let report = pool::build_pool_doctor_report(&db, &docker_ctl, &client).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_pool_doctor_report(&report);
            }
            if !report.ok {
                anyhow::bail!("pool doctor found unhealthy runner pool state");
            }
        }
        PoolCommands::Repair {
            yes,
            prune_stale,
            json,
        } => {
            if !yes {
                anyhow::bail!("refusing to repair pools without --yes");
            }
            let report = pool::repair_pool_state(
                &db,
                &docker_ctl,
                &client,
                pool::PoolRepairOptions { prune_stale },
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("pool repair: {} action(s)", report.actions.len());
                for action in &report.actions {
                    println!("  - {action}");
                }
                print_pool_doctor_report(&report.doctor);
            }
            if !report.doctor.ok {
                anyhow::bail!("pool repair completed but pool doctor still reports issues");
            }
        }
        PoolCommands::Scale { name, count } => {
            let started = pool::scale_pool_to(&db, &docker_ctl, &client, &name, count).await?;
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
        PoolCommands::Remove { name } => {
            pool::delete_pool(&db, &docker_ctl, &client, &name).await?;
            println!("✅ Pool '{}' deleted", name);
        }
        PoolCommands::RotateToken { name } => {
            let new_token = pool::rotate_pool_token(&db, &docker_ctl, &client, &name).await?;
            println!(
                "🔑 Pool '{}' token rotated: {}...{}",
                name,
                &new_token[..8],
                &new_token[new_token.len() - 4..]
            );
        }
    }
    Ok(())
}

fn print_pool_doctor_report(report: &pool::PoolDoctorReport) {
    println!("━━━ jeryu pool doctor ━━━");
    println!("  Status: {}", if report.ok { "ok" } else { "blocked" });
    println!("  Issues: {}", report.issues.len());
    if let Some(topology) = &report.topology {
        println!(
            "  Standard topology: active={} desired={}",
            topology.active_total, topology.desired_total
        );
        for entry in &topology.entries {
            println!(
                "    {:<10} active={:<3} desired={:<3} delta={}",
                entry.node_alias, entry.active, entry.desired, entry.delta
            );
        }
    }
    if !report.reserved_nodes.is_empty() {
        println!("  Reserved nodes:");
        for node in &report.reserved_nodes {
            println!(
                "    {:<10} active={:<3} desired={:<3} {}",
                node.node_alias, node.active, node.desired, node.reason
            );
        }
    }
    if !report.issues.is_empty() {
        println!();
        println!("  Findings:");
        for issue in &report.issues {
            let scope = issue
                .pool
                .as_deref()
                .or(issue.node_alias.as_deref())
                .or(issue.manager_id.as_deref())
                .unwrap_or("-");
            println!(
                "    [{}] {} {}: {}",
                issue.severity, issue.code, scope, issue.message
            );
        }
    }
}
