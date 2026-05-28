//! Owner: Runner Fleet / Pool Management
//! Proof: `cargo test -p jeryu -- pool`
//! Invariants: Pool→Manager is 1:N; SIGQUIT for graceful drain; SIGHUP for token hot-reload
//!
//! A pool is a logical runner configuration in GitLab backed by
//! 0-N runner-manager containers on the local Docker host.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use tracing::{info, warn};

use crate::config;
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::state::{Db, Pool};
use tokio::time::{Duration, Instant, sleep};

#[path = "pool_ops.rs"]
mod pool_ops;
pub(crate) use pool_ops::wait_for_active_managers;
pub use pool_ops::{delete_pool, drain_pool, pause_pool, resume_pool, rotate_pool_token};

#[path = "pool_scale.rs"]
mod pool_scale;
pub use pool_scale::{
    count_running_managers, reconcile_manager_runtime_state, scale_pool_to,
    scale_standard_pool_topology,
};

#[path = "pool_topology.rs"]
mod pool_topology;
pub use pool_topology::*;

#[path = "pool_doctor.rs"]
mod pool_doctor;
pub use pool_doctor::*;

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
            cluster_alias: None,
            backend_type: "docker".into(),
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

pub async fn repair_standard_pool_tags(store: &Db) -> Result<usize> {
    let pools = store.list_pools().await?;
    let mut repaired = 0;

    for pool in pools
        .iter()
        .filter(|pool| crate::runner_policy::is_standard_pool_name(&pool.name))
        .filter(|pool| !pool.tags.trim().is_empty())
    {
        store.update_pool_tags(&pool.name, "").await?;
        repaired += 1;
        warn!(
            pool = %pool.name,
            previous_tags = %pool.tags,
            "cleared stale tags from standard runner pool row"
        );
    }

    Ok(repaired)
}

pub async fn repair_standard_pool_capacity(store: &Db) -> Result<usize> {
    let pools = store.list_pools().await?;
    let mut repaired = 0;

    for pool in pools
        .iter()
        .filter(|pool| crate::runner_policy::is_standard_pool_name(&pool.name))
    {
        let desired = if pool.name == config::STANDARD_POOL_NAME {
            config::STANDARD_POOL_DESIRED_TOTAL
        } else {
            0
        };

        if pool.min_warm != desired || pool.max_managers != desired {
            store
                .update_pool_capacity(&pool.name, desired, desired)
                .await?;
            repaired += 1;
            warn!(
                pool = %pool.name,
                min_warm = desired,
                max_managers = desired,
                "repaired standard runner pool capacity"
            );
        }

        if pool.name == config::STANDARD_POOL_NAME && pool.backend_type != "docker-remote" {
            store
                .update_pool_backend(config::STANDARD_POOL_NAME, "docker-remote")
                .await?;
            repaired += 1;
            warn!(
                pool = config::STANDARD_POOL_NAME,
                "repaired standard runner pool backend for explicit topology"
            );
        }
    }

    Ok(repaired)
}

// ---------------------------------------------------------------------------
// Pause / Resume
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(name: &str, tags: &str) -> Pool {
        Pool {
            name: name.to_string(),
            gitlab_runner_id: 1,
            auth_token: "token".into(),
            tags: tags.to_string(),
            executor: "docker".into(),
            min_warm: 1,
            max_managers: 1,
            concurrent: 1,
            request_concurrency: 1,
            paused: false,
            trust_tier: "trusted".into(),
            cluster_alias: None,
            backend_type: "docker".into(),
        }
    }

    #[tokio::test]
    async fn stale_standard_pool_tags_are_repaired_to_empty() -> Result<()> {
        let db = Db::open_memory().await?;
        db.insert_pool(&pool(config::STANDARD_POOL_NAME, "ci,stale"))
            .await?;

        let repaired = repair_standard_pool_tags(&db).await?;

        assert_eq!(repaired, 1);
        assert_eq!(
            db.get_pool(config::STANDARD_POOL_NAME)
                .await?
                .expect("pool")
                .tags,
            ""
        );
        Ok(())
    }

    #[tokio::test]
    async fn standard_pool_capacity_repair_sets_topology_total() -> Result<()> {
        let db = Db::open_memory().await?;
        db.insert_pool(&pool(config::STANDARD_POOL_NAME, ""))
            .await?;

        let repaired = repair_standard_pool_capacity(&db).await?;

        assert_eq!(repaired, 2);
        let pool = db
            .get_pool(config::STANDARD_POOL_NAME)
            .await?
            .expect("pool");
        assert_eq!(pool.min_warm, config::STANDARD_POOL_DESIRED_TOTAL);
        assert_eq!(pool.max_managers, config::STANDARD_POOL_DESIRED_TOTAL);
        assert_eq!(pool.backend_type, "docker-remote");
        Ok(())
    }
}
