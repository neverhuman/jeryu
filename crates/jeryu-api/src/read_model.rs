//! Live read-model assembly: turns [`ForgeCore`] state into the [`TuiReadModel`]
//! the TUI/web panes render. Kept out of `web.rs` so the HTTP/WS edge stays
//! focused on routing rather than rollup logic.

use jeryu_core::{CheckConclusion, CheckRunStatus, ForgeCore, PullRequestState};
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
/// into a single synthetic `default` [`PoolRollup`] so the Pools/Health pane has
/// a real, non-empty fabric to render. [`SystemHealth`] reports every component
/// (`scm`/`database`/`sandbox`/`cache`/`vault`) as Healthy because holding a
/// live `ForgeCore` means the local plane is open and serving.
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
        let checks = core
            .list_check_runs(&repo.owner, &repo.name, None)
            .map(|runs| runs.check_runs)
            .unwrap_or_default();

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

        // A repo with neither open PRs nor any check-run is not active work; skip
        // it so the activity rollup reflects real load rather than every repo.
        let open_pulls = core
            .list_pull_requests(&repo.owner, &repo.name, None)
            .map(|pulls| {
                pulls
                    .iter()
                    .filter(|pr| {
                        !matches!(
                            pr.state,
                            PullRequestState::Closed | PullRequestState::Merged
                        )
                    })
                    .count() as u32
            })
            .unwrap_or(0);
        if open_pulls == 0 && checks.is_empty() {
            continue;
        }

        default_pool.queued_jobs = default_pool.queued_jobs.saturating_add(queued);
        default_pool.running_jobs = default_pool.running_jobs.saturating_add(running);
        default_pool.failed_jobs = default_pool.failed_jobs.saturating_add(failed);

        repos.push(RepoActivity {
            repo: repo.full_name.clone(),
            queued_jobs: queued,
            running_jobs: running,
            failed_jobs: failed,
            pools: vec!["default".to_string()],
        });
    }

    // Size the synthetic pool's capacity to the running load so utilization is
    // meaningful and the pool only shows saturated when work is genuinely queued
    // with no idle slot. With no work at all, leave a single idle slot.
    default_pool.active_slots = default_pool.running_jobs.max(1);
    default_pool.configured_max_slots = default_pool.active_slots;
    default_pool.online_runners = default_pool.active_slots;

    // Only surface the pool once there is at least one active repo; an empty
    // server yields an empty (Unknown-health) activity rollup, never a fake pool.
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
    SystemHealth {
        scm: ComponentHealth::ok("scm", 0),
        database: ComponentHealth::ok("database", 0),
        sandbox: ComponentHealth::ok("sandbox", 0),
        cache: ComponentHealth::ok("cache", 0),
        vault: ComponentHealth::ok("vault", 0),
        runners: RunnerHealth::default(),
    }
}
