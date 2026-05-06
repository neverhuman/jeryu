use crate::cli::PoolCommands;
use anyhow::Result;
use jeryu::{docker, pool, state};

pub(crate) async fn execute_pool_commands(subcmd: PoolCommands) -> Result<()> {
    let (client, _) = crate::dispatch::load_client()?;
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
