//! Builder/fixture for a populated sample [`TuiReadModel`].
//!
//! Used by `--demo`, screenshots, and serde round-trip tests. The builder is
//! deterministic given a fixed `generated_at` so snapshots are stable.

use chrono::{DateTime, TimeZone, Utc};

use crate::dashboards::runners::{RunnersDashboard, RunnersItem, RunnersSummary};
use crate::dashboards::source_doctor::{
    SourceDoctorDashboard, SourceDoctorItem, SourceDoctorSummary,
};
use crate::entity::{ActionRef, BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::freshness::{FreshnessState, SourceFreshness, SourceKind};
use crate::health::{ComponentHealth, RunnerHealth};
use crate::queue::{QueueJobSummary, QueuePoolSnapshot, QueueSnapshot};
use crate::read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, SystemHealth,
    TuiReadModel,
};
use crate::repos::{RepoSummary, ReposSnapshot};
use crate::risk::RiskTier;

/// Fluent builder for a sample read model.
#[derive(Debug, Clone)]
pub struct TuiReadModelBuilder {
    model: TuiReadModel,
}

impl TuiReadModelBuilder {
    /// Start from the crate default (empty, healthy) snapshot.
    pub fn new() -> Self {
        Self {
            model: TuiReadModel::default(),
        }
    }

    /// Pin `generated_at` for deterministic fixtures/snapshots.
    pub fn generated_at(mut self, at: DateTime<Utc>) -> Self {
        self.model.generated_at = at;
        self
    }

    pub fn event_cursor(mut self, cursor: u64) -> Self {
        self.model.event_cursor = cursor;
        self
    }

    pub fn mission(mut self, mission: MissionSnapshot) -> Self {
        self.model.mission = mission;
        self
    }

    pub fn queue(mut self, queue: QueueSnapshot) -> Self {
        self.model.queue = queue;
        self
    }

    pub fn repos(mut self, repos: ReposSnapshot) -> Self {
        self.model.repos = repos;
        self
    }

    pub fn runners(mut self, runners: RunnersDashboard) -> Self {
        self.model.runners = runners;
        self
    }

    pub fn source_doctor(mut self, sd: SourceDoctorDashboard) -> Self {
        self.model.source_doctor = sd;
        self
    }

    pub fn attention(mut self, items: Vec<AttentionItem>) -> Self {
        self.model.attention = items;
        self
    }

    pub fn next_action(mut self, action: NextActionRecommendation) -> Self {
        self.model.next_action = Some(action);
        self
    }

    pub fn system(mut self, system: SystemHealth) -> Self {
        self.model.system = system;
        self
    }

    pub fn build(self) -> TuiReadModel {
        self.model
    }
}

impl Default for TuiReadModelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A fully-populated, deterministic sample read model exercising every nested
/// contract type. Suitable for serde round-trip tests and demo rendering.
pub fn sample_read_model() -> TuiReadModel {
    let at: DateTime<Utc> = Utc
        .with_ymd_and_hms(2026, 5, 30, 12, 0, 0)
        .single()
        .unwrap();

    let mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "ci_failure".into(),
            severity: Severity::Error,
            summary: "build-web failing on core/web".into(),
            entity: Some(EntityRef::new(EntityKind::Job, "build-web")),
            recommended_action: Some(ActionRef::new("retry_job", "Retry build-web", RiskTier::R2)),
        }),
        active_agents: 4,
        blocked_agents: 1,
        running_jobs: 3,
        failed_jobs: 1,
        queued_jobs: 2,
        open_capsules: 5,
        active_grants: 2,
        cache_hit_ratio: 0.87,
        active_taints: 0,
        selector_misses_24h: 12,
        agents_can_code: true,
        active_runners: 6,
        total_runners: 8,
        evidence_count: 17,
        taint_count: 0,
    };

    let queue = QueueSnapshot {
        total_waiting_jobs: 2,
        total_running_jobs: 3,
        pools: vec![{
            let mut p = QueuePoolSnapshot::new("trusted");
            p.active_managers = 2;
            p.max_managers = 4;
            p.slots_per_manager = 2;
            p.running_jobs = 3;
            p
        }],
        waiting_jobs: vec![{
            let mut j = QueueJobSummary::new("build-web", 12_000);
            j.ci_run = Some(EntityRef::new(EntityKind::CiRun, "ci-run/9001"));
            j
        }],
    };

    let repos = ReposSnapshot::from_repo_summaries(
        "/var/lib/jeryu/registry",
        vec![{
            let mut r = RepoSummary::new("web", "core/web");
            r.provider = "neutral".into();
            r.status = "green".into();
            r.running_count = 1;
            r
        }],
    );

    let runners = RunnersDashboard {
        items: vec![RunnersItem {
            id: "runner-1".into(),
            label: "oci-runner-1".into(),
            runner_id: "r-1".into(),
            pool: "trusted".into(),
            status: HealthLevel::Healthy,
            tags: vec!["oci".into(), "linux".into()],
            last_seen: Some(at),
        }],
        freshness: Some(SourceFreshness::live(SourceKind::Scm, at, "cursor-1")),
        summary: Some(RunnersSummary {
            total_runners: 8,
            active_runners: 6,
            paused_runners: 1,
            draining_runners: 1,
        }),
    };

    let source_doctor = SourceDoctorDashboard {
        items: vec![SourceDoctorItem {
            id: "scm".into(),
            label: "Source control".into(),
            source_kind: SourceKind::Scm,
            state: "live".into(),
            last_error: None,
            last_observed_at: Some(at),
            drift_kind: None,
        }],
        freshness: Some(SourceFreshness {
            source: SourceKind::Scm,
            state: FreshnessState::Fresh,
            observed_at: Some(at),
            age_ms: Some(250),
            cursor: Some("cursor-1".into()),
            ttl_ms: Some(5_000),
            confidence: 0.99,
            last_error: None,
            degraded_reason: None,
        }),
        summary: Some(SourceDoctorSummary {
            sources_total: 5,
            sources_healthy: 4,
            sources_degraded: 1,
            schema_drift_count: 0,
        }),
    };

    let attention = vec![AttentionItem {
        id: "att-1".into(),
        severity: Severity::Error,
        title: "build-web failing".into(),
        why_it_matters: "Blocks the merge gate for core/web".into(),
        entity: EntityRef::new(EntityKind::Job, "build-web"),
        evidence: vec!["evidence/cap-17".into()],
        recommended_actions: vec![ActionRef::new("retry_job", "Retry build-web", RiskTier::R2)],
        created_at: at,
        last_seen_at: at,
    }];

    let next_action = NextActionRecommendation {
        action_ref: ActionRef::new("retry_job", "Retry build-web", RiskTier::R2),
        label: "Retry the failing build".into(),
        why: "A transient failure is blocking the merge gate".into(),
        entity: Some(EntityRef::new(EntityKind::Job, "build-web")),
        confidence: 0.82,
        safety: ActionSafety::Reversible,
        risk: RiskTier::R2,
    };

    let system = SystemHealth {
        scm: ComponentHealth::ok("scm", 12),
        database: ComponentHealth::ok("database", 3),
        sandbox: ComponentHealth::ok("sandbox", 8),
        cache: ComponentHealth::ok("cache", 4),
        vault: ComponentHealth::ok("vault", 6),
        runners: RunnerHealth {
            online: 6,
            busy: 3,
            idle: 3,
            degraded: 0,
        },
    };

    TuiReadModelBuilder::new()
        .generated_at(at)
        .event_cursor(42)
        .mission(mission)
        .queue(queue)
        .repos(repos)
        .runners(runners)
        .source_doctor(source_doctor)
        .attention(attention)
        .next_action(next_action)
        .system(system)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_populated() {
        let m = sample_read_model();
        assert_eq!(m.event_cursor, 42);
        assert_eq!(m.mission.active_agents, 4);
        assert!(m.next_action.is_some());
        assert_eq!(m.system.scm.name, "scm");
        assert_eq!(m.runners.items.len(), 1);
        assert_eq!(m.source_doctor.items[0].source_kind, SourceKind::Scm);
    }

    #[test]
    fn builder_overrides_defaults() {
        let m = TuiReadModelBuilder::new().event_cursor(7).build();
        assert_eq!(m.event_cursor, 7);
        assert_eq!(m.schema_version, "tui.v1.0");
    }
}
