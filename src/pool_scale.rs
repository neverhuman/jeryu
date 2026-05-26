use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use tracing::{info, warn};

use crate::config;
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::state::{Db, Manager, Pool};
use tokio::time::Duration;

use super::wait_for_active_managers;

#[path = "pool_scale_support.rs"]
mod support;
pub(crate) use support::*;

pub async fn reconcile_manager_runtime_state(
    store: &Db,
    docker: &DockerCtl,
    pool_name: Option<&str>,
) -> Result<usize> {
    let running_container_ids = docker.running_managed_container_ids().await?;
    let managers = store.list_managers(pool_name).await?; // allowlist: pool orchestration owns runner state
    let mut stopped = 0;

    for manager in managers
        .iter()
        .filter(|manager| manager_state_counts_as_active(&manager.state))
        .filter(|manager| !manager_has_running_container(manager, &running_container_ids))
    {
        warn!(
            manager_id = %manager.id,
            pool = %manager.pool_name,
            container_id = %manager.docker_container_id,
            previous_state = %manager.state,
            "marking expired runner manager stopped; Docker container is not running"
        );
        store.update_manager_state(&manager.id, "stopped").await?; // allowlist: pool orchestration owns runner state
        stopped += 1;
    }

    Ok(stopped)
}

pub async fn count_running_managers(
    store: &Db,
    docker: &DockerCtl,
    pool_name: &str,
) -> Result<i64> {
    let running_container_ids = docker.running_managed_container_ids().await?;
    let managers = store.list_managers(Some(pool_name)).await?; // allowlist: pool orchestration owns runner state
    Ok(managers
        .iter()
        .filter(|manager| manager_state_counts_as_active(&manager.state))
        .filter(|manager| manager_has_running_container(manager, &running_container_ids))
        .count() as i64)
}

pub(crate) async fn remove_manager_cache_dir(docker: &DockerCtl, manager_id: &str) {
    let cache_dir = config::manager_cache_dir(manager_id);
    if !cache_dir.exists() {
        return;
    }
    if let Err(err) = docker.remove_cache_dir_as_root(&cache_dir).await {
        warn!(manager_id, path = %cache_dir.display(), error = %err, "failed to remove manager cache dir");
    }
}

#[allow(dead_code)]
async fn start_manager(store: &Db, docker: &DockerCtl, pool: &Pool, pool_name: &str) -> Result<()> {
    let manager_id = uuid::Uuid::new_v4().to_string();
    let config_dir = config::runners_dir()
        .join(&manager_id)
        .display()
        .to_string();
    let manager_cache_dir = config::manager_cache_dir(&manager_id);
    let pool_cache_dir = config::pool_cache_root(pool_name);
    let pool_targets_dir = config::pool_cargo_targets_root(pool_name);
    let pool_sccache_dir = config::pool_cargo_sccache_dir(pool_name);

    fs::create_dir_all(&config_dir)
        .with_context(|| format!("creating config dir: {config_dir}"))?;
    fs::create_dir_all(&manager_cache_dir)
        .with_context(|| format!("creating cache dir: {}", manager_cache_dir.display()))?;
    fs::create_dir_all(&pool_targets_dir)
        .with_context(|| format!("creating pool targets dir: {}", pool_targets_dir.display()))?;
    fs::create_dir_all(&pool_sccache_dir)
        .with_context(|| format!("creating pool sccache dir: {}", pool_sccache_dir.display()))?;

    let gitlab_url = format!(
        "http://{}:{}",
        config::GITLAB_HOSTNAME,
        config::GITLAB_HTTP_PORT
    );
    let config_content = config::render_runner_config(
        pool_name,
        &manager_id,
        &gitlab_url,
        &pool.auth_token,
        &pool.executor,
        &pool_cache_dir.display().to_string(),
        pool.concurrent,
        pool.request_concurrency,
    );
    fs::write(format!("{config_dir}/config.toml"), &config_content)?;

    let container_id = docker
        .start_runner_manager(
            &manager_id,
            &config_dir,
            &manager_cache_dir.display().to_string(),
            &pool_cache_dir.display().to_string(),
            &pool.executor,
            None,
        )
        .await
        .with_context(|| format!("starting manager for pool '{pool_name}'"))?;

    let manager = Manager {
        id: manager_id.clone(),
        pool_name: pool_name.to_string(),
        docker_container_id: container_id,
        system_id: None,
        state: "starting".to_string(),
        config_dir,
        started_at: Some(chrono::Utc::now().to_rfc3339()),
        last_contact_at: None,
        node_alias: None, // local Docker
    };
    store.insert_manager(&manager).await?; // allowlist: pool orchestration owns runner state

    info!(manager_id, pool = pool_name, "started new manager");
    Ok(())
}

/// Scale a pool to exactly `target` active managers. Returns the number
/// of managers started (may be 0 if already at target or scaling down).
pub async fn scale_pool_to(
    store: &Db,
    docker: &DockerCtl,
    _client: &GitlabClient,
    pool_name: &str,
    target: usize,
) -> Result<usize> {
    let pool = match store.get_pool(pool_name).await? {
        Some(pool) => pool,
        None => return Err(anyhow::anyhow!("pool '{}' not found", pool_name)),
    };

    reconcile_manager_runtime_state(store, docker, Some(pool_name)).await?;
    let active = store.count_active_managers(pool_name).await? as usize; // allowlist: pool orchestration owns runner state

    if active == target {
        info!(pool = pool_name, active, target, "pool already at target");
        return Ok(0);
    }

    if active > target {
        // Scale down: drain excess managers
        let excess = active - target;
        let managers = store.list_managers(Some(pool_name)).await?; // allowlist: pool orchestration owns runner state
        let to_drain: Vec<_> = managers
            .iter()
            .filter(|m| m.state == "online" || m.state == "starting")
            .take(excess)
            .collect();

        for m in &to_drain {
            info!(manager_id = %m.id, pool = pool_name, "draining excess manager");
            store.update_manager_state(&m.id, "draining").await?; // allowlist: pool orchestration owns runner state
            stop_manager_for_node(
                docker,
                m,
                config::runner_shutdown_timeout_secs() as i64,
            )
            .await
            .ok(); // best-effort drain
            // Only clean up local cache dirs (remote managers manage their own storage).
            if m.node_alias.is_none() {
                remove_manager_cache_dir(docker, &m.id).await;
            }
            store.update_manager_state(&m.id, "stopped").await?; // allowlist: pool orchestration owns runner state
        }

        let active_after_drain = store.count_active_managers(pool_name).await? as usize; // allowlist: pool orchestration owns runner state
        if active_after_drain < target {
            for _ in 0..(target - active_after_drain) {
                start_pool_manager(store, docker, &pool, pool_name).await?;
            }
        }
        wait_for_active_managers(store, pool_name, target as i64, Duration::from_secs(90)).await?;
        return Ok(0);
    }

    // Scale up: start new managers
    crate::cache::ensure_root_disk_headroom(
        crate::cache::ROOT_DISK_HEADROOM_MIN_FREE_BYTES,
        "runner fanout",
    )
    .await?;
    let to_start = target - active;
    let mut started = 0;

    for _ in 0..to_start {
        start_pool_manager(store, docker, &pool, pool_name).await?;
        started += 1;
    }

    wait_for_active_managers(store, pool_name, target as i64, Duration::from_secs(90)).await?;
    Ok(started)
}
