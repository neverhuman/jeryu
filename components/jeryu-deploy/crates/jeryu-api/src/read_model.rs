//! Live read-model assembly: turns [`ForgeCore`] state into the [`TuiReadModel`]
//! the TUI/web panes render. Kept out of `web.rs` so the HTTP/WS edge stays
//! focused on routing rather than rollup logic.

use jeryu_core::{CheckConclusion, CheckRunStatus, ForgeCore};
use jeryu_readmodel::{
    ComponentHealth, FreshnessState, HealthLevel, PoolActivity, PoolRollup, RepoActivity,
    RunnerHealth, SourceFreshness, SourceKind, SystemHealth, TuiReadModel,
};

/// Build a populated [`TuiReadModel`] from live [`ForgeCore`] state.
///
/// For every repository on the server we roll up its open pull requests and
/// check-runs into a [`RepoActivity`], classifying each check-run by status:
/// `Queued` → queued, `InProgress` → running, and any `Completed` run whose
/// conclusion is `Failure` → failed. The per-repo counts are then aggregated
/// into a `default` [`PoolRollup`]. No registered runner fleet is connected to
/// this assembler, so capacity remains unverified and its source freshness is
/// explicitly Unknown. Existing check activity is preserved independently.
pub(crate) fn assemble_read_model(core: &ForgeCore) -> TuiReadModel {
    TuiReadModel {
        pool_activity: assemble_pool_activity(core),
        system: assemble_system_health(),
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

    // Keep observed jobs while leaving unverified capacity at its zero defaults.
    // These counts do not establish a registered runner or an available slot.
    let pools = if repos.is_empty() {
        Vec::new()
    } else {
        vec![default_pool]
    };

    PoolActivity {
        repos,
        pools,
        freshness: Some(SourceFreshness {
            source: SourceKind::Broker,
            state: FreshnessState::Unknown,
            observed_at: None,
            age_ms: None,
            cursor: None,
            ttl_ms: None,
            confidence: 0.0,
            last_error: None,
            degraded_reason: Some("runner capacity registry is not connected".to_string()),
        }),
        ..PoolActivity::default()
    }
}

/// No component probes or runner registry are connected to this assembler.
/// Loading forge state cannot establish the health or latency of these services.
fn assemble_system_health() -> SystemHealth {
    SystemHealth {
        scm: unverified_component("scm"),
        database: unverified_component("database"),
        sandbox: unverified_component("sandbox"),
        cache: unverified_component("cache"),
        vault: unverified_component("vault"),
        runners: RunnerHealth::default(),
    }
}

fn unverified_component(name: &str) -> ComponentHealth {
    ComponentHealth {
        status: HealthLevel::Unknown,
        ..ComponentHealth::unknown(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_server_has_unknown_runner_capacity() {
        let model = assemble_read_model(&ForgeCore::new());
        assert!(model.pool_activity.repos.is_empty());
        assert!(model.pool_activity.pools.is_empty());
        let freshness = model.pool_activity.freshness.unwrap();
        assert_eq!(freshness.state, FreshnessState::Unknown);
        assert_eq!(freshness.source, SourceKind::Broker);
        assert!(freshness.observed_at.is_none());
        assert_eq!(freshness.confidence, 0.0);
        assert_eq!(
            freshness.degraded_reason.as_deref(),
            Some("runner capacity registry is not connected")
        );
    }

    #[test]
    fn runner_health_does_not_invent_registered_runners() {
        let system = assemble_system_health();
        assert_eq!(system.runners, RunnerHealth::default());
        for component in [
            &system.scm,
            &system.database,
            &system.sandbox,
            &system.cache,
            &system.vault,
        ] {
            assert_eq!(component.status, HealthLevel::Unknown);
            assert_eq!(component.latency_ms, None);
            assert_eq!(component.detail.as_deref(), Some("not yet checked"));
        }
    }

    #[test]
    fn pool_preserves_observed_jobs_without_claiming_runner_capacity() {
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
        for (name, status, conclusion) in [
            ("queued", CheckRunStatus::Queued, None),
            ("running", CheckRunStatus::InProgress, None),
            (
                "failed",
                CheckRunStatus::Completed,
                Some(CheckConclusion::Failure),
            ),
        ] {
            core.create_check_run(
                "alice",
                "jeryu",
                jeryu_core::CreateCheckRunRequest {
                    name: name.to_string(),
                    head_sha: "a".repeat(40),
                    status: Some(status),
                    conclusion,
                    ..jeryu_core::CreateCheckRunRequest::default()
                },
            )
            .unwrap();
        }
        let activity = assemble_pool_activity(&core);
        assert_eq!(activity.repos.len(), 1, "the tracked repo is surfaced");
        assert!(!activity.pools.is_empty(), "a tracked repo surfaces a pool");
        let pool = &activity.pools[0];
        assert_eq!(pool.configured_max_slots, 0);
        assert_eq!(pool.active_slots, 0);
        assert_eq!(pool.online_runners, 0);
        assert_eq!(pool.stuck_runners, 0);
        assert_eq!(
            (pool.queued_jobs, pool.running_jobs, pool.failed_jobs),
            (1, 1, 1)
        );
        let repo = &activity.repos[0];
        assert_eq!(
            (repo.queued_jobs, repo.running_jobs, repo.failed_jobs),
            (1, 1, 1)
        );
        assert_eq!(activity.freshness.unwrap().state, FreshnessState::Unknown);
    }
}
