//! Per-dashboard sample snapshots used by the populated read-model fixture.

use chrono::{DateTime, TimeZone, Utc};

use crate::dashboards::agents::{AgentItem, AgentStatus, AgentsSnapshot, AgentsSummary};
use crate::dashboards::approvals::{
    ApprovalItem, ApprovalsSnapshot, ApprovalsSummary, CheckStatus,
};
use crate::dashboards::evidence::{EvidenceItem, EvidenceSnapshot, EvidenceSummary, GateDecision};
use crate::dashboards::release::{
    PromotionStage, ReleaseGate, ReleaseItem, ReleaseSnapshot, ReleaseSummary, SbomStatus,
};
use crate::dashboards::workflow::{
    DeliveryPosture, WorkflowItem, WorkflowSnapshot, WorkflowSummary,
};
use crate::entity::{EntityKind, EntityRef, HealthLevel};
use crate::freshness::{SourceFreshness, SourceKind};
use crate::risk::RiskTier;

/// Deterministic timestamp shared by every sample fixture in this module.
pub(super) fn sample_at() -> DateTime<Utc> {
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
