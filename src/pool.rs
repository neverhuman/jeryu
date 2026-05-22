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
pub use pool_scale::{count_running_managers, reconcile_manager_runtime_state, scale_pool_to};

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
// Pause / Resume
// ---------------------------------------------------------------------------
