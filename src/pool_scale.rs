use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::Arc;
use tracing::{info, warn};

use crate::config;
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::runner_backend::{ManagedContainerSnapshot, RunnerBackend};
use crate::runner_backend_local::LocalDockerBackend;
use crate::runner_backend_remote::RemoteDockerBackend;
use crate::state::{Db, Manager, Pool};
use futures_util::future::join_all;
use tokio::time::Duration;

use super::wait_for_active_managers;

#[path = "pool_scale_support.rs"]
mod support;
pub(crate) use support::*;

#[derive(Debug, Default)]
struct LiveInventory {
    containers: Vec<ManagedContainerSnapshot>,
    unreachable_nodes: BTreeSet<String>,
}

async fn collect_live_inventory(docker: &DockerCtl) -> Result<LiveInventory> {
    let mut backends: Vec<(Option<String>, Arc<dyn RunnerBackend>)> = Vec::new();
    backends.push((
        Some(crate::config::LOCAL_NODE_ALIAS.to_string()),
        Arc::new(LocalDockerBackend::new(docker.clone())),
    ));

    let node_configs = crate::node_support::list_node_configs()?;
    for node in node_configs.into_iter().filter(|node| node.enabled) {
        backends.push((
            Some(node.alias.clone()),
            Arc::new(RemoteDockerBackend::new(node)),
        ));
    }

    let mut live = LiveInventory::default();
    let results = join_all(backends.into_iter().map(|(alias, backend)| async move {
        let result = backend.list_managed_containers().await;
        (alias, result)
    }))
    .await;

    for (alias, result) in results {
        match result {
            Ok(mut containers) => live.containers.append(&mut containers),
            Err(err) => {
                if alias.as_deref() == Some(crate::config::LOCAL_NODE_ALIAS) {
                    return Err(err);
                }
                if let Some(alias) = alias {
                    warn!(node = %alias, error = %err, "could not inspect remote node inventory");
                    live.unreachable_nodes.insert(alias);
                }
            }
        }
    }

    Ok(live)
}

fn manager_matches_snapshot(manager: &Manager, snapshot: &ManagedContainerSnapshot) -> bool {
    manager.id == snapshot.manager_id || manager.docker_container_id == snapshot.container_id
}

fn manager_config_dir_from_snapshot(snapshot: &ManagedContainerSnapshot) -> Result<String> {
    if let Some(alias) = snapshot.node_alias.as_deref()
        && alias != crate::config::LOCAL_NODE_ALIAS
    {
        let node_cfg = crate::node_support::load_node_config(alias)?;
        return Ok(format!(
            "{}/{}",
            node_cfg.runner_data_dir, snapshot.manager_id
        ));
    }

    Ok(config::runners_dir()
        .join(&snapshot.manager_id)
        .display()
        .to_string())
}

async fn rehydrate_missing_manager(store: &Db, snapshot: &ManagedContainerSnapshot) -> Result<()> {
    let config_dir = match manager_config_dir_from_snapshot(snapshot) {
        Ok(path) => path,
        Err(err) => {
            warn!(
                manager_id = %snapshot.manager_id,
                node = ?snapshot.node_alias,
                error = %err,
                "could not derive config dir for live manager snapshot"
            );
            return Ok(());
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    let manager = Manager {
        id: snapshot.manager_id.clone(),
        pool_name: snapshot.pool_name.clone(),
        docker_container_id: snapshot.container_id.clone(),
        system_id: None,
        state: if snapshot.running {
            "online".to_string()
        } else {
            "stopped".to_string()
        },
        config_dir,
        started_at: Some(now.clone()),
        last_contact_at: snapshot.running.then_some(now),
        node_alias: snapshot
            .node_alias
            .as_deref()
            .filter(|alias| *alias != crate::config::LOCAL_NODE_ALIAS)
            .map(str::to_string),
    };
    store.insert_manager(&manager).await?;
    Ok(())
}

async fn reconcile_snapshot(
    store: &Db,
    manager: &Manager,
    snapshot: &ManagedContainerSnapshot,
    docker: &DockerCtl,
) -> Result<()> {
    if snapshot.running {
        if manager.node_alias.is_none()
            && let Err(err) = docker
                .ensure_runner_manager_restart_policy(&manager.docker_container_id)
                .await
        {
            warn!(
                manager_id = %manager.id,
                container_id = %manager.docker_container_id,
                error = %err,
                "failed to enforce local runner restart policy"
            );
        }
        if manager.state != "draining" && manager.state != "online" {
            store.update_manager_state(&manager.id, "online").await?;
        }
        if manager.state != "draining" {
            let now = chrono::Utc::now().to_rfc3339();
            store.update_manager_last_contact(&manager.id, &now).await?;
        }
    } else if manager_state_counts_as_active(&manager.state) {
        warn!(
            manager_id = %manager.id,
            pool = %manager.pool_name,
            node = ?manager.node_alias,
            container_id = %manager.docker_container_id,
            previous_state = %manager.state,
            "marking expired runner manager stopped; Docker inventory says it is gone"
        );
        store.update_manager_state(&manager.id, "stopped").await?;
    }

    Ok(())
}

pub async fn reconcile_manager_runtime_state(
    store: &Db,
    docker: &DockerCtl,
    pool_name: Option<&str>,
) -> Result<usize> {
    let _lease = match pool_name {
        Some(pool_name) => {
            Some(super::PoolOrchestrationLeaseGuard::acquire(store, pool_name).await?)
        }
        None => None,
    };
    reconcile_manager_runtime_state_inner(store, docker, pool_name).await
}

pub(crate) async fn reconcile_manager_runtime_state_inner(
    store: &Db,
    docker: &DockerCtl,
    pool_name: Option<&str>,
) -> Result<usize> {
    let live = collect_live_inventory(docker).await?;
    let managers = store.list_managers(pool_name).await?; // allowlist: pool orchestration owns runner state
    let manager_by_id: BTreeMap<String, Manager> = managers
        .iter()
        .cloned()
        .map(|manager| (manager.id.clone(), manager))
        .collect();
    let seen_manager_ids: BTreeSet<String> = live
        .containers
        .iter()
        .map(|snapshot| snapshot.manager_id.clone())
        .collect();
    let unreachable_nodes = live.unreachable_nodes.clone();
    let mut stopped = 0;

    for alias in &unreachable_nodes {
        warn!(
            node = %alias,
            "could not inspect remote node; marking managers unreachable"
        );
        store.mark_node_managers_unreachable(alias).await?;
    }

    for snapshot in live
        .containers
        .iter()
        .filter(|snapshot| pool_name.is_none_or(|pool| snapshot.pool_name == pool))
    {
        if let Some(manager) = manager_by_id.get(&snapshot.manager_id) {
            if manager_matches_snapshot(manager, snapshot) {
                reconcile_snapshot(store, manager, snapshot, docker).await?;
                if !snapshot.running && manager_state_counts_as_active(&manager.state) {
                    stopped += 1;
                }
            } else if snapshot.running {
                warn!(
                    manager_id = %manager.id,
                    pool = %manager.pool_name,
                    live_container_id = %snapshot.container_id,
                    stored_container_id = %manager.docker_container_id,
                    "live manager snapshot container id differs from DB row"
                );
            }
        } else if snapshot.running {
            rehydrate_missing_manager(store, snapshot).await?;
        }
    }

    for manager in managers.iter().filter(|manager| {
        manager_state_counts_as_active(&manager.state)
            && !seen_manager_ids.contains(&manager.id)
            && manager
                .node_alias
                .as_deref()
                .map(|alias| !unreachable_nodes.contains(alias))
                .unwrap_or(true)
    }) {
        warn!(
            manager_id = %manager.id,
            pool = %manager.pool_name,
            container_id = %manager.docker_container_id,
            previous_state = %manager.state,
            "marking expired runner manager stopped; Docker inventory does not contain it"
        );
        store.update_manager_state(&manager.id, "stopped").await?;
        stopped += 1;
    }

    Ok(stopped)
}

pub async fn count_running_managers(
    _store: &Db,
    docker: &DockerCtl,
    pool_name: &str,
) -> Result<i64> {
    let live = collect_live_inventory(docker).await?;
    Ok(live
        .containers
        .iter()
        .filter(|snapshot| snapshot.pool_name == pool_name && snapshot.running)
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
        .start_runner_manager(crate::docker::RunnerManagerStartSpec {
            manager_id: &manager_id,
            pool_name,
            config_dir: &config_dir,
            manager_cache_dir: &manager_cache_dir.display().to_string(),
            pool_cache_dir: &pool_cache_dir.display().to_string(),
            executor: &pool.executor,
            docker_socket: None,
        })
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
    let _lease = super::PoolOrchestrationLeaseGuard::acquire(store, pool_name).await?;

    reconcile_manager_runtime_state_inner(store, docker, Some(pool_name)).await?;
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
            .filter(|m| manager_state_counts_as_active(&m.state))
            .take(excess)
            .collect();

        for m in &to_drain {
            info!(manager_id = %m.id, pool = pool_name, "draining excess manager");
            store.update_manager_state(&m.id, "draining").await?; // allowlist: pool orchestration owns runner state
            stop_manager_for_node(docker, m, config::runner_shutdown_timeout_secs() as i64)
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

pub async fn scale_standard_pool_topology(
    store: &Db,
    docker: &DockerCtl,
    _client: &GitlabClient,
    pool_name: &str,
) -> Result<usize> {
    if !super::pool_topology::is_standard_topology_pool(pool_name) {
        let pool = store
            .get_pool(pool_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("pool '{}' not found", pool_name))?;
        return scale_pool_to(store, docker, _client, pool_name, pool.min_warm as usize).await;
    }

    let pool = store
        .get_pool(pool_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pool '{}' not found", pool_name))?;
    let _lease = super::PoolOrchestrationLeaseGuard::acquire(store, pool_name).await?;

    reconcile_manager_runtime_state_inner(store, docker, Some(pool_name)).await?;
    let managers = store.list_managers(Some(pool_name)).await?;
    let targets = super::pool_topology::desired_standard_pool_targets();
    let plan = super::pool_topology::plan_pool_topology(pool_name, &managers, &targets);

    for entry in plan.entries.iter().filter(|entry| entry.delta < 0) {
        let excess = entry.delta.unsigned_abs();
        let to_drain = managers
            .iter()
            .filter(|manager| manager_state_counts_as_active(&manager.state))
            .filter(|manager| super::pool_topology::manager_node_key(manager) == entry.node_alias)
            .take(excess)
            .collect::<Vec<_>>();

        for manager in to_drain {
            info!(
                manager_id = %manager.id,
                pool = pool_name,
                node = %entry.node_alias,
                "draining manager outside desired standard pool topology"
            );
            store.update_manager_state(&manager.id, "draining").await?;
            stop_manager_for_node(
                docker,
                manager,
                config::runner_shutdown_timeout_secs() as i64,
            )
            .await
            .ok();
            if manager.node_alias.is_none() {
                remove_manager_cache_dir(docker, &manager.id).await;
            }
            store.update_manager_state(&manager.id, "stopped").await?;
        }
    }

    let managers = store.list_managers(Some(pool_name)).await?;
    let plan = super::pool_topology::plan_pool_topology(pool_name, &managers, &targets);
    let to_start = plan
        .entries
        .iter()
        .filter(|entry| entry.delta > 0)
        .map(|entry| entry.delta as usize)
        .sum::<usize>();

    if to_start > 0 {
        crate::cache::ensure_root_disk_headroom(
            crate::cache::ROOT_DISK_HEADROOM_MIN_FREE_BYTES,
            "standard runner topology fanout",
        )
        .await?;
    }

    let mut started = 0;
    for entry in plan.entries.iter().filter(|entry| entry.delta > 0) {
        let node_alias = if entry.node_alias == config::LOCAL_NODE_ALIAS {
            None
        } else {
            Some(entry.node_alias.as_str())
        };
        for _ in 0..(entry.delta as usize) {
            start_pool_manager_on_node(store, docker, &pool, pool_name, node_alias).await?;
            started += 1;
        }
    }

    wait_for_active_managers(
        store,
        pool_name,
        super::pool_topology::standard_pool_desired_total() as i64,
        Duration::from_secs(90),
    )
    .await?;
    Ok(started)
}
