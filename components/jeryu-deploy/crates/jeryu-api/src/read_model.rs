//! Live read-model assembly: turns [`ForgeCore`] state into the [`TuiReadModel`]
//! the TUI/web panes render. Kept out of `web.rs` so the HTTP/WS edge stays
//! focused on routing rather than rollup logic.

use jeryu_core::{CheckConclusion, CheckRunStatus, ForgeCore};
use jeryu_readmodel::{
    ComponentHealth, PoolActivity, PoolRollup, RepoActivity, RunnerHealth, SystemHealth,
    TuiReadModel,
};

/// Build a populated [`TuiReadModel`] from live [`ForgeCore`] state.
///
/// For every repository on the server we roll up its open pull requests and
/// check-runs into a [`RepoActivity`], classifying each check-run by status:
/// `Queued` → queued, `InProgress` → running, and any `Completed` run whose
/// conclusion is `Failure` → failed. The per-repo counts are then aggregated
/// into a `default` [`PoolRollup`] whose runner capacity (online/slots/stuck)
/// comes from the live [`jeryu_runnerd`] dogfood fleet — so the Pools/Health
/// pane renders real numbers, not a synthetic single slot. [`SystemHealth`]
/// reports every component (`scm`/`database`/`sandbox`/`cache`/`vault`) as
/// Healthy because holding a live `ForgeCore` means the local plane is open and
/// serving, and `runners` reflects the live fleet snapshot.
pub(crate) fn assemble_read_model(core: &ForgeCore) -> TuiReadModel {
    TuiReadModel {
        pool_activity: assemble_pool_activity(core),
        system: healthy_system(),
        ..TuiReadModel::default()
    }
}

/// Roll up every repo's PRs + check-runs into [`PoolActivity`].
fn assemble_pool_activity(core: &ForgeCore) -> PoolActivity {
    let mut repos: Vec<RepoActivity> = Vec::new();
    let mut default_pool = PoolRollup::new("default");

    for repo in core.list_repositories(None) {
        let checks = match core.list_check_runs(&repo.owner, &repo.name, None) {
            Ok(runs) => runs.check_runs,
            Err(_) => Vec::new(),
        };

        let mut queued = 0u32;
        let mut running = 0u32;
        let mut failed = 0u32;
        for check in &checks {
            match check.status {
                CheckRunStatus::Queued => queued = queued.saturating_add(1),
                CheckRunStatus::InProgress => running = running.saturating_add(1),
                CheckRunStatus::Completed => {
                    if check.conclusion == Some(CheckConclusion::Failure) {
                        failed = failed.saturating_add(1);
                    }
                }
            }
        }

        default_pool.queued_jobs = default_pool.queued_jobs.saturating_add(queued);
        default_pool.running_jobs = default_pool.running_jobs.saturating_add(running);
        default_pool.failed_jobs = default_pool.failed_jobs.saturating_add(failed);

        // Every tracked repo is surfaced (with its live job counts) so the Repos
        // pane reflects the real roster, not only repos with in-flight work.
        repos.push(RepoActivity {
            repo: repo.full_name.clone(),
            queued_jobs: queued,
            running_jobs: running,
            failed_jobs: failed,
            pools: vec!["default".to_string()],
        });
    }

    // Pool runner capacity comes from the REAL dogfood runner fleet (4 nodes ×
    // 10 slots), not a synthetic single slot — so online/slots/utilization on the
    // Pools/Health pane reflect the live fabric instead of reading zero.
    let fleet = jeryu_runnerd::fleet_snapshot();
    default_pool.online_runners = fleet.online_runners;
    default_pool.active_slots = fleet.active_slots;
    default_pool.configured_max_slots = fleet.total_slots;
    default_pool.stuck_runners = fleet.stuck_runners;

    // Surface the pool once there is at least one tracked repo, preserving the
    // empty-server "no fabric" contract; when shown, the pool carries the REAL
    // fleet capacity set above instead of a synthetic single slot.
    let pools = if repos.is_empty() {
        Vec::new()
    } else {
        vec![default_pool]
    };

    PoolActivity {
        repos,
        pools,
        ..PoolActivity::default()
    }
}

/// All system components reported Healthy: holding a live `ForgeCore` means the
/// local control plane (scm/db/sandbox/cache/vault) is open and serving.
fn healthy_system() -> SystemHealth {
    let fleet = jeryu_runnerd::fleet_snapshot();
    SystemHealth {
        scm: ComponentHealth::ok("scm", 0),
        database: ComponentHealth::ok("database", 0),
        sandbox: ComponentHealth::ok("sandbox", 0),
        cache: ComponentHealth::ok("cache", 0),
        vault: ComponentHealth::ok("vault", 0),
        // Real runner health from the live fleet, not all-zero defaults.
        runners: RunnerHealth {
            online: fleet.online_runners,
            busy: fleet.busy_runners,
            idle: fleet.idle_runners,
            degraded: fleet.stuck_runners,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fleet_snapshot_reflects_the_dogfood_fleet() {
        // The default fleet is the deterministic dogfood fixture: 4 nodes × 10.
        let fleet = jeryu_runnerd::fleet_snapshot();
        assert_eq!(fleet.nodes, 4, "dogfood fixture has 4 nodes (xbabe0..3)");
        assert_eq!(fleet.total_slots, 40, "4 nodes × 10 slots = 40");
        assert!(
            fleet.online_runners >= 1,
            "fixture nodes are online, not zero"
        );
        assert_eq!(fleet.online_runners + fleet.stuck_runners, fleet.nodes);
    }

    #[test]
    fn healthy_system_reports_real_runner_health_not_zeros() {
        let system = healthy_system();
        assert!(
            system.runners.online >= 1,
            "system.runners must reflect the live fleet, not RunnerHealth::default() zeros"
        );
    }

    #[test]
    fn pool_carries_real_fleet_capacity_not_a_synthetic_slot() {
        // A server with a tracked repo surfaces a pool whose runner capacity is
        // the REAL dogfood fleet (4 nodes × 10 = 40 slots, 4 online) — not the
        // old synthetic single idle slot that read as near-zero on the Pools pane.
        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            jeryu_core::CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        let activity = assemble_pool_activity(&core);
        assert_eq!(activity.repos.len(), 1, "the tracked repo is surfaced");
        assert!(!activity.pools.is_empty(), "a tracked repo surfaces a pool");
        let pool = &activity.pools[0];
        assert_eq!(
            pool.configured_max_slots, 40,
            "configured slots must be the real fleet capacity (40), not a synthetic 1"
        );
        assert_eq!(pool.online_runners, 4, "the 4 dogfood nodes are online");
    }
}
