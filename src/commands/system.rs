use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{config, docker, gitlab_client, pool, release, secrets, state};

pub async fn execute_down() -> Result<()> {
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
    Ok(())
}

pub fn execute_status() -> Result<()> {
    println!("━━━ JeRyu Status ━━━\n");
    let git_path = match std::env::var("JERYU_SYSTEM_GIT") {
        Ok(path) => path,
        Err(_) => "/usr/bin/git".into(),
    };
    std::process::Command::new(&git_path)
        .args(["status"])
        .status()?;
    Ok(())
}

pub async fn execute_system_status() -> Result<()> {
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

    let managed = match docker_ctl.list_managed_containers().await {
        Ok(c) => c,
        Err(_) => Vec::new(),
    };
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

    let events = match db.recent_job_events(5).await {
        Ok(e) => e,
        Err(_) => Vec::new(),
    };
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

    if let Ok(Some(secret_set)) = db
        .latest_release_secret_set(&crate::cli::infer_repo_name())
        .await
    {
        println!("\n  Latest release secret set:");
        println!(
            "    {} {} [{}] {}",
            secret_set.version, secret_set.target, secret_set.status, secret_set.authority_name
        );
    }

    println!();
    Ok(())
}
