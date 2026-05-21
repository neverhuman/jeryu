//! Owner: Runner Fleet / Pool Management
//! Proof: `cargo test -p jeryu -- pool`
//! Invariants: Pool→Manager is 1:N; SIGQUIT for graceful drain; SIGHUP for token hot-reload
//!
//! A pool is a logical runner configuration in GitLab backed by
//! 0-N runner-manager containers on the local Docker host.

use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use tracing::{info, warn};

use crate::config;
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::state::{Db, Manager, Pool};
use tokio::time::{Duration, Instant, sleep};

#[path = "pool_ops.rs"]
mod pool_ops;
pub(crate) use pool_ops::wait_for_active_managers;
pub use pool_ops::{delete_pool, drain_pool, pause_pool, resume_pool, rotate_pool_token};

fn runner_token_env_key(pool_name: &str) -> String {
    format!("RUNNER_TOKEN_{}", pool_name.to_ascii_uppercase())
}

/// Rehydrate default pool rows after local state loss when GitLab runner
/// registrations and auth tokens still exist.
pub async fn ensure_default_pool_rows(store: &Db, client: &GitlabClient) -> Result<usize> {
    let existing: HashSet<String> = store
        .list_pools()
        .await?
        .into_iter()
        .map(|pool| pool.name)
        .collect();
    let missing: Vec<_> = config::DEFAULT_POOLS
        .iter()
        .filter(|pool_def| !existing.contains(pool_def.name))
        .collect();

    if missing.is_empty() {
        return Ok(0);
    }

    let runners = client
        .list_all_runners()
        .await
        .context("listing GitLab runners while repairing pool state")?;
    let mut inserted = 0;

    for pool_def in missing {
        let description = format!("jeryu-{}", pool_def.name);
        let Some(runner) = runners
            .iter()
            .find(|runner| runner.description.as_deref() == Some(description.as_str()))
        else {
            warn!(
                pool = pool_def.name,
                runner = %description,
                "default pool row is missing and no matching GitLab runner registration exists"
            );
            continue;
        };

        let env_key = runner_token_env_key(pool_def.name);
        let Ok(auth_token) = std::env::var(&env_key) else {
            warn!(
                pool = pool_def.name,
                env_key,
                runner_id = runner.id,
                "default pool row is missing but the runner auth token is absent"
            );
            continue;
        };

        let pool = Pool {
            name: pool_def.name.to_string(),
            gitlab_runner_id: runner.id,
            auth_token,
            tags: pool_def.tags.to_string(),
            executor: pool_def.executor.to_string(),
            min_warm: pool_def.min_warm,
            max_managers: pool_def.max_managers,
            concurrent: pool_def.concurrent,
            request_concurrency: pool_def.request_concurrency,
            paused: runner.paused.unwrap_or(false),
            trust_tier: pool_def.trust_tier.to_string(),
        };
        store.insert_pool(&pool).await?;
        inserted += 1;
        info!(
            pool = pool_def.name,
            runner_id = runner.id,
            "repaired missing default pool row from GitLab runner registration"
        );
    }

    Ok(inserted)
}

// ---------------------------------------------------------------------------
// Scale: bring manager count to target
// ---------------------------------------------------------------------------

fn manager_state_counts_as_active(state: &str) -> bool {
    matches!(state, "starting" | "online")
}

fn manager_has_running_container(
    manager: &Manager,
    running_container_ids: &BTreeSet<String>,
) -> bool {
    running_container_ids.contains(&manager.docker_container_id)
}

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

async fn remove_manager_cache_dir(docker: &DockerCtl, manager_id: &str) {
    let cache_dir = config::manager_cache_dir(manager_id);
    if !cache_dir.exists() {
        return;
    }
    if let Err(err) = docker.remove_cache_dir_as_root(&cache_dir).await {
        warn!(manager_id, path = %cache_dir.display(), error = %err, "failed to remove manager cache dir");
    }
}

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
            docker
                .cleanup_runner_cache(&m.docker_container_id)
                .await
                .ok();
            docker
                .drain_runner_manager(
                    &m.docker_container_id,
                    config::runner_shutdown_timeout_secs() as i64,
                )
                .await
                .ok(); // best-effort drain
            docker
                .cleanup_runner_cache(&m.docker_container_id)
                .await
                .ok();
            docker
                .remove_runner_manager(&m.docker_container_id)
                .await
                .ok();
            remove_manager_cache_dir(docker, &m.id).await;
            store.update_manager_state(&m.id, "stopped").await?; // allowlist: pool orchestration owns runner state
        }

        let active_after_drain = store.count_active_managers(pool_name).await? as usize; // allowlist: pool orchestration owns runner state
        if active_after_drain < target {
            for _ in 0..(target - active_after_drain) {
                start_manager(store, docker, &pool, pool_name).await?;
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
        start_manager(store, docker, &pool, pool_name).await?;
        started += 1;
    }

    wait_for_active_managers(store, pool_name, target as i64, Duration::from_secs(90)).await?;
    Ok(started)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(state: &str, docker_container_id: &str) -> Manager {
        Manager {
            id: "manager-id".into(),
            pool_name: "default".into(),
            docker_container_id: docker_container_id.into(),
            system_id: None,
            state: state.into(),
            config_dir: "/tmp/manager".into(),
            started_at: None,
            last_contact_at: None,
        }
    }

    #[test]
    fn active_manager_requires_running_container() {
        let running = BTreeSet::from(["container-a".to_string()]);
        assert!(manager_has_running_container(
            &manager("online", "container-a"),
            &running
        ));
        assert!(!manager_has_running_container(
            &manager("online", "container-b"),
            &running
        ));
    }

    #[test]
    fn only_starting_and_online_count_as_active_states() {
        assert!(manager_state_counts_as_active("starting"));
        assert!(manager_state_counts_as_active("online"));
        assert!(!manager_state_counts_as_active("draining"));
        assert!(!manager_state_counts_as_active("stopped"));
        assert!(!manager_state_counts_as_active("failed"));
    }

    #[test]
    fn runner_token_env_key_matches_bootstrap_keys() {
        assert_eq!(runner_token_env_key("default"), "RUNNER_TOKEN_DEFAULT");
        assert_eq!(runner_token_env_key("build"), "RUNNER_TOKEN_BUILD");
        assert_eq!(runner_token_env_key("untrusted"), "RUNNER_TOKEN_UNTRUSTED");
    }
}

// ---------------------------------------------------------------------------
// Pause / Resume
// ---------------------------------------------------------------------------
