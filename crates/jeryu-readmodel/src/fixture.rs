//! Builder/fixture for a populated sample [`TuiReadModel`].
//!
//! Used by `--demo`, screenshots, and serde round-trip tests. The builder is
//! deterministic given a fixed `generated_at` so snapshots are stable.

use chrono::{DateTime, TimeZone, Utc};

use crate::dashboards::agents::{AgentItem, AgentStatus, AgentsSnapshot, AgentsSummary};
use crate::dashboards::approvals::{
    ApprovalItem, ApprovalsSnapshot, ApprovalsSummary, CheckStatus,
};
use crate::dashboards::evidence::{EvidenceItem, EvidenceSnapshot, EvidenceSummary, GateDecision};
use crate::dashboards::release::{
    PromotionStage, ReleaseGate, ReleaseItem, ReleaseSnapshot, ReleaseSummary, SbomStatus,
};
use crate::dashboards::runners::{RunnersDashboard, RunnersItem, RunnersSummary};
use crate::dashboards::source_doctor::{
    SourceDoctorDashboard, SourceDoctorItem, SourceDoctorSummary,
};
use crate::dashboards::workflow::{
    DeliveryPosture, WorkflowItem, WorkflowSnapshot, WorkflowSummary,
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

    pub fn approvals(mut self, approvals: ApprovalsSnapshot) -> Self {
        self.model.approvals = approvals;
        self
    }

    pub fn evidence(mut self, evidence: EvidenceSnapshot) -> Self {
        self.model.evidence = evidence;
        self
    }

    pub fn agents(mut self, agents: AgentsSnapshot) -> Self {
        self.model.agents = agents;
        self
    }

    pub fn release(mut self, release: ReleaseSnapshot) -> Self {
        self.model.release = release;
        self
    }

    pub fn workflow(mut self, workflow: WorkflowSnapshot) -> Self {
        self.model.workflow = workflow;
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

/// Deterministic timestamp shared by every sample fixture in this module.
fn sample_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 30, 12, 0, 0)
        .single()
        .unwrap()
}

/// A populated approvals snapshot: two pending PRs (one passing, one red,
/// high-risk) plus a roll-up summary. GitHub PR shape (numbers + checks).
pub fn sample_approvals() -> ApprovalsSnapshot {
    let at = sample_at();
    let mut passing = ApprovalItem::new(101, "fix flaky integration test", RiskTier::R2);
    passing.author = "agent-wrath-17".into();
    passing.checks = CheckStatus::Success;
    passing.age = "3m".into();
    passing.head_sha = "0badc0ffee1234".into();

    let mut risky = ApprovalItem::new(102, "risky schema migration", RiskTier::R4);
    risky.author = "agent-storm-04".into();
    risky.checks = CheckStatus::Failure;
    risky.age = "47m".into();
    risky.head_sha = "deadbeefcafef00d".into();

    ApprovalsSnapshot {
        items: vec![passing, risky],
        freshness: Some(SourceFreshness::live(SourceKind::Scm, at, "cursor-1")),
        summary: Some(ApprovalsSummary {
            pending_total: 2,
            checks_passing: 1,
            checks_failing: 1,
            high_risk_count: 1,
        }),
    }
}

/// A populated evidence snapshot: an allow receipt and a deny receipt.
pub fn sample_evidence() -> EvidenceSnapshot {
    let at = sample_at();
    let mut allow = EvidenceItem::new(
        "cap-17",
        EntityRef::new(EntityKind::PullRequest, "101"),
        GateDecision::Allow,
    );
    allow.label = "merge gate satisfied".into();
    allow.recorded_at = Some(at);

    let mut deny = EvidenceItem::new(
        "cap-18",
        EntityRef::new(EntityKind::ReleaseGate, "rel-1"),
        GateDecision::Deny,
    );
    deny.label = "release gate denied: SBOM missing".into();
    deny.recorded_at = Some(at);
    deny.redacted = true;

    EvidenceSnapshot {
        items: vec![allow, deny],
        freshness: Some(SourceFreshness::live(
            SourceKind::InspectionHttp,
            at,
            "cursor-1",
        )),
        summary: Some(EvidenceSummary {
            total_capsules: 17,
            open_capsules: 5,
            denied_count: 1,
            redacted_count: 1,
        }),
    }
}

/// A populated agents snapshot: an active, a blocked, and an idle session.
pub fn sample_agents() -> AgentsSnapshot {
    let at = sample_at();
    let mut active = AgentItem::new("agent-wrath-17", AgentStatus::Active);
    active.current_task = Some("implement approvals lens".into());
    active.branch = Some("feat/approvals".into());
    active.grants = 2;

    let mut blocked = AgentItem::new("agent-storm-04", AgentStatus::Blocked);
    blocked.current_task = Some("awaiting human review on PR 102".into());
    blocked.grants = 1;

    let idle = AgentItem::new("agent-calm-09", AgentStatus::Idle);

    AgentsSnapshot {
        items: vec![active, blocked, idle],
        freshness: Some(SourceFreshness::live(SourceKind::Autonomy, at, "cursor-1")),
        summary: Some(AgentsSummary {
            total_sessions: 3,
            active_sessions: 1,
            blocked_sessions: 1,
            active_grants: 3,
            agents_can_code: true,
        }),
    }
}

/// A populated release snapshot: a ready candidate and a blocked one.
pub fn sample_release() -> ReleaseSnapshot {
    let at = sample_at();
    let mut ready = ReleaseItem::new("rel-1", "abc1234");
    ready.label = "core v2.4.0-rc1".into();
    ready.gate = ReleaseGate::Ready;
    ready.stage = PromotionStage::Canary;
    ready.sbom = SbomStatus::Verified;
    ready.rollback_target = Some("v2.3.9".into());

    let mut blocked = ReleaseItem::new("rel-2", "def5678");
    blocked.label = "web v1.9.0-rc3".into();
    blocked.gate = ReleaseGate::Blocked;
    blocked.stage = PromotionStage::Candidate;
    blocked.sbom = SbomStatus::Missing;

    ReleaseSnapshot {
        items: vec![ready, blocked],
        freshness: Some(SourceFreshness::live(
            SourceKind::ArtifactStore,
            at,
            "cursor-1",
        )),
        summary: Some(ReleaseSummary {
            candidate_ready: true,
            canary_passing: true,
            production_health: HealthLevel::Healthy,
            blocked_count: 1,
        }),
    }
}

/// A populated workflow snapshot: a running and a blocked delivery pipeline.
pub fn sample_workflow() -> WorkflowSnapshot {
    let at = sample_at();
    let mut running = WorkflowItem::new("pipe-9001", "core/web");
    running.label = "core/web delivery".into();
    running.pr_number = Some(101);
    running.posture = DeliveryPosture::Running;
    running.critical_path_node = Some("ci:build-web".into());

    let mut blocked = WorkflowItem::new("pipe-9002", "core/api");
    blocked.label = "core/api delivery".into();
    blocked.pr_number = Some(102);
    blocked.posture = DeliveryPosture::Blocked;
    blocked.critical_path_node = Some("gate:approval".into());

    WorkflowSnapshot {
        items: vec![running, blocked],
        freshness: Some(SourceFreshness::live(SourceKind::Scm, at, "cursor-1")),
        summary: Some(WorkflowSummary {
            total_pipelines: 2,
            running_count: 1,
            blocked_count: 1,
            longest_running_seconds: 2_840,
        }),
    }
}

/// A fully-populated, deterministic sample read model exercising every nested
/// contract type. Suitable for serde round-trip tests and demo rendering.
pub fn sample_read_model() -> TuiReadModel {
    let at: DateTime<Utc> = sample_at();

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
        .approvals(sample_approvals())
        .evidence(sample_evidence())
        .agents(sample_agents())
        .release(sample_release())
        .workflow(sample_workflow())
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
        assert_eq!(m.approvals.items.len(), 2);
        assert_eq!(m.approvals.failing_checks(), 1);
        assert_eq!(m.evidence.denied(), 1);
        assert_eq!(m.agents.blocked(), 1);
        assert_eq!(m.release.blocked(), 1);
        assert_eq!(m.workflow.blocked(), 1);
    }

    #[test]
    fn builder_overrides_defaults() {
        let m = TuiReadModelBuilder::new().event_cursor(7).build();
        assert_eq!(m.event_cursor, 7);
        assert_eq!(m.schema_version, "tui.v1.0");
    }
}
