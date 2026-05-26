use anyhow::Result;
use std::collections::BTreeSet;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use super::{EngineState, SharedState};
use crate::pool;
use crate::release;
use crate::state::Pool as RunnerPool;

pub(crate) async fn check_scale_up(state: &EngineState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let queued = state.db.count_queued_jobs().await?;
    let running = state.db.count_running_jobs().await?;

    for p in &pools {
        if p.paused {
            continue;
        }

        let active = state.db.count_active_managers(&p.name).await?;
        let target = desired_manager_target(p, queued, running);

        if active < target {
            info!(
                pool = %p.name,
                active,
                target,
                queued,
                running,
                min_warm = p.min_warm,
                "scaling up to meet queue demand"
            );
            pool::scale_pool_to(
                &state.db,
                &state.docker,
                &state.client,
                &p.name,
                target as usize,
            )
            .await?;
        }
    }

    Ok(())
}

pub(crate) async fn reconciliation_loop(state: SharedState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));

    loop {
        interval.tick().await;
        debug!("running reconciliation");

        if let Err(e) = reconcile_once(&state).await {
            error!(error = %e, "reconciliation failed");
        }
    }
}

pub(crate) async fn reconcile_once(state: &EngineState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let queued = state.db.count_queued_jobs().await?;
    let running = state.db.count_running_jobs().await?;

    for p in &pools {
        if p.paused {
            continue;
        }

        // Reconcile local managers (existing behavior, unchanged).
        let _stale_managers =
            pool::reconcile_manager_runtime_state(&state.db, &state.docker, Some(&p.name)).await?;

        // Reconcile remote managers: group by node_alias, one SSH call per node.
        reconcile_remote_managers_for_pool(state, &p.name).await;

        let active = state.db.count_active_managers(&p.name).await?;

        let target = desired_manager_target(p, queued, running);

        if active != target {
            info!(
                pool = %p.name,
                active,
                target,
                queued,
                running,
                "reconciler: scaling pool"
            );
            pool::scale_pool_to(
                &state.db,
                &state.docker,
                &state.client,
                &p.name,
                target as usize,
            )
            .await?;
        }

        let managers = state.db.list_managers(Some(&p.name)).await?;
        for m in &managers {
            if m.system_id.is_none() && (m.state == "starting" || m.state == "online") {
                // Only probe local managers for system_id (remote managers don't
                // have a directly accessible config_dir on the control-plane host).
                if m.node_alias.is_none() {
                    let system_id_path = format!("{}/.runner_system_id", m.config_dir);
                    if let Ok(sid) = std::fs::read_to_string(&system_id_path) {
                        let sid = sid.trim().to_string();
                        if !sid.is_empty() {
                            info!(manager_id = %m.id, system_id = %sid, "discovered system_id");
                            state.db.update_manager_system_id(&m.id, &sid).await?;
                            state.db.update_manager_state(&m.id, "online").await?;
                        }
                    }
                }
            }
        }
    }

    if let Err(err) = release::reconcile_release_for_ref(
        &state.db,
        &state.client,
        release::DEFAULT_RELEASE_PROJECT_ID,
        "main",
        false,
    )
    .await
    {
        warn!(error = %err, "release reconciliation failed");
    }

    match crate::repo_local::reconcile_repo_sidecars(&state.db).await {
        Ok(runs) => {
            for run in runs {
                debug!(
                    repo = %run.repo,
                    status = %run.status,
                    detail = %run.detail,
                    "repo sidecar reconciliation completed"
                );
            }
        }
        Err(err) => {
            warn!(error = %err, "repo sidecar reconciliation failed");
        }
    }

    // Per-node storage GC — run at most once per hour per node.
    gc_remote_nodes(state).await;

    Ok(())
}

/// Run storage GC on each registered remote node, rate-limited to once per hour.
async fn gc_remote_nodes(state: &EngineState) {
    const GC_INTERVAL_SECS: u64 = 3600; // 1 hour

    let node_configs = match crate::node_support::list_node_configs() {
        Ok(cfgs) => cfgs,
        Err(e) => {
            warn!(error = %e, "failed to load node configs for GC");
            return;
        }
    };

    for cfg in &node_configs {
        if !cfg.enabled {
            continue;
        }

        // Check rate limit.
        let should_gc = {
            let timestamps = state.node_gc_timestamps.lock().unwrap();
            match timestamps.get(&cfg.alias) {
                None => true,
                Some(last) => last.elapsed().as_secs() >= GC_INTERVAL_SECS,
            }
        };

        if !should_gc {
            continue;
        }

        let alias = cfg.alias.clone();
        let limit_gb = cfg.storage_limit_gb;

        if let Some(backend) = state.backend_registry.get_by_alias(&alias) {
            debug!(node = %alias, limit_gb, "running storage GC");
            match backend.gc_storage(limit_gb).await {
                Ok(()) => {
                    let mut timestamps = state.node_gc_timestamps.lock().unwrap();
                    timestamps.insert(alias.clone(), Instant::now());
                }
                Err(e) => {
                    warn!(node = %alias, error = %e, "storage GC failed");
                }
            }
        }
    }
}

fn desired_manager_target(pool: &RunnerPool, queued: i64, running: i64) -> i64 {
    let queue_target = pool.min_warm.saturating_add(queued).max(pool.min_warm);
    let active_work_target = queue_target.max(running);
    active_work_target.clamp(pool.min_warm, pool.max_managers)
}

/// Reconcile managers on remote nodes for a pool.
///
/// Groups managers by `node_alias`, makes one SSH call per node to get the
/// running container IDs, then syncs DB states:
/// - Container running      → ensure state is `online` (or `node_starting` → `online`)
/// - Container gone         → set state to `stopped`
/// - SSH unreachable        → set state to `node_unreachable` (don't assume dead)
async fn reconcile_remote_managers_for_pool(state: &EngineState, pool_name: &str) {
    let managers = match state.db.list_managers(Some(pool_name)).await {
        Ok(m) => m,
        Err(e) => {
            warn!(pool = pool_name, error = %e, "failed to list managers for remote reconciliation");
            return;
        }
    };

    // Collect unique node aliases for managers in active-ish states.
    let node_aliases: std::collections::HashSet<String> = managers
        .iter()
        .filter(|m| m.node_alias.is_some())
        .filter(|m| {
            matches!(
                m.state.as_str(),
                "starting" | "online" | "node_starting" | "node_unreachable" | "draining"
            )
        })
        .filter_map(|m| m.node_alias.clone())
        .collect();

    for alias in &node_aliases {
        let backend = match state.backend_registry.get_by_alias(alias) {
            Some(b) => b,
            None => {
                warn!(node = %alias, "no backend registered for node alias; skipping reconcile");
                continue;
            }
        };

        let running_ids: BTreeSet<String> = match backend.list_running_backend_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                warn!(node = %alias, error = %e, "SSH failed; marking managers node_unreachable");
                if let Err(db_err) = state.db.mark_node_managers_unreachable(alias).await {
                    warn!(node = %alias, error = %db_err, "failed to mark managers unreachable in DB");
                }
                continue;
            }
        };

        // Sync each manager on this node.
        for m in managers.iter().filter(|m| m.node_alias.as_deref() == Some(alias.as_str())) {
            let is_running = running_ids.contains(&m.docker_container_id);

            match m.state.as_str() {
                "node_starting" | "node_unreachable" if is_running => {
                    info!(
                        manager_id = %m.id,
                        node = %alias,
                        "remote manager recovered and running; marking online"
                    );
                    let _ = state.db.update_manager_state(&m.id, "online").await;
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = state.db.update_manager_last_contact(&m.id, &now).await;
                }
                "online" | "starting" | "node_starting" if !is_running => {
                    warn!(
                        manager_id = %m.id,
                        node = %alias,
                        "remote manager container gone; marking stopped"
                    );
                    let _ = state.db.update_manager_state(&m.id, "stopped").await;
                }
                "online" if is_running => {
                    // Healthy — just refresh last_contact_at.
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = state.db.update_manager_last_contact(&m.id, &now).await;
                }
                _ => {} // draining, stopped, failed — leave alone
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(min_warm: i64, max_managers: i64) -> RunnerPool {
        RunnerPool {
            name: "build".into(),
            gitlab_runner_id: 1,
            auth_token: "token".into(),
            tags: "build".into(),
            executor: "docker".into(),
            min_warm,
            max_managers,
            concurrent: 8,
            request_concurrency: 4,
            paused: false,
            trust_tier: "trusted".into(),
            cluster_alias: None,
            backend_type: "docker".into(),
        }
    }

    #[test]
    fn desired_manager_target_accounts_for_running_jobs() {
        let pool = pool(1, 3);

        assert_eq!(desired_manager_target(&pool, 0, 0), 1);
        assert_eq!(desired_manager_target(&pool, 1, 0), 2);
        assert_eq!(desired_manager_target(&pool, 0, 3), 3);
        assert_eq!(desired_manager_target(&pool, 4, 4), 3);
    }
}
