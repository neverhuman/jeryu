use anyhow::Result;
use tracing::{debug, error, info, warn};

use super::{EngineState, SharedState};
use crate::pool;
use crate::release;
use crate::state::Pool as RunnerPool;

#[path = "engine_background_reconcile_remote.rs"]
mod remote;
use remote::gc_remote_nodes;

pub(crate) async fn check_scale_up(state: &EngineState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let queued = state.db.count_queued_jobs().await?;
    let running = state.db.count_running_jobs().await?;

    if let Ok(normalized_runners) =
        crate::runner_policy::enforce_pool_runner_policy(&state.client, &pools).await
        && normalized_runners > 0
    {
        info!(
            normalized_runners,
            "normalized GitLab runners to pool policy during reconciliation"
        );
    }

    for p in &pools {
        if p.paused {
            continue;
        }

        pool::reconcile_manager_runtime_state(&state.db, &state.docker, Some(&p.name)).await?;
        let active = state.db.count_active_managers(&p.name).await?;
        let target = desired_manager_target(p, queued, running);

        if pool::is_standard_topology_pool(&p.name) {
            let desired = pool::standard_pool_desired_total() as i64;
            if active < desired {
                info!(
                    pool = %p.name,
                    active,
                    target = desired,
                    "scaling standard pool up to configured topology"
                );
                pool::scale_standard_pool_topology(
                    &state.db,
                    &state.docker,
                    &state.client,
                    &p.name,
                )
                .await?;
            }
        } else if active < target {
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

        let active = state.db.count_active_managers(&p.name).await?;

        let target = if pool::is_standard_topology_pool(&p.name) {
            pool::standard_pool_desired_total() as i64
        } else {
            desired_manager_target(p, queued, running)
        };

        if active != target {
            info!(
                pool = %p.name,
                active,
                target,
                queued,
                running,
                "reconciler: scaling pool"
            );
            if pool::is_standard_topology_pool(&p.name) {
                pool::scale_standard_pool_topology(
                    &state.db,
                    &state.docker,
                    &state.client,
                    &p.name,
                )
                .await?;
            } else {
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

fn desired_manager_target(pool: &RunnerPool, queued: i64, running: i64) -> i64 {
    let queue_target = pool.min_warm.saturating_add(queued).max(pool.min_warm);
    let active_work_target = queue_target.max(running);
    active_work_target.clamp(pool.min_warm, pool.max_managers)
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
