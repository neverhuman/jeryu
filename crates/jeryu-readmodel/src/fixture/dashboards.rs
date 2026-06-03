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
use crate::dashboards::workcells::{
    WorkcellItem, WorkcellState, WorkcellsDashboard, WorkcellsSummary,
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

/// A populated workcells snapshot: one claimed workcell, one held repair cell,
/// and one blocked workcell. This covers claim state, repo roots, branch
/// budget, git status, CI snapshot age, runner epoch, and heartbeat health.
pub fn sample_workcells() -> WorkcellsDashboard {
    let at = sample_at();
    let mut claimed = WorkcellItem::new("wc-17", "agent-wrath-17 / core/web");
    claimed.claim_state = WorkcellState::Claimed;
    claimed.agent_id = "agent-wrath-17".into();
    claimed.repo_roots = vec!["/workspace/core/web".into()];
    claimed.workspace_root = "/workspace/core/web".into();
    claimed.branch_budget = 1;
    claimed.branches_open = 1;
    claimed.git_status_summary = "1 modified, 0 untracked".into();
    claimed.ci_snapshot_age_ms = Some(120_000);
    claimed.runner_id = "xbabe0".into();
    claimed.runner_epoch = 7;
    claimed.heartbeat_healthy = true;
    claimed.startup_rebased = true;
    claimed.startup_main_ref = Some("origin/main".into());
    claimed.startup_base_sha = Some("abc123".into());
    claimed.startup_head_sha = Some("def456".into());

    let mut held = WorkcellItem::new("wc-18", "agent-storm-04 / core/api");
    held.claim_state = WorkcellState::Held;
    held.agent_id = "agent-storm-04".into();
    held.repo_roots = vec!["/workspace/core/api".into()];
    held.workspace_root = "/workspace/core/api".into();
    held.branch_budget = 5;
    held.branches_open = 0;
    held.git_status_summary = "held after failed tree capture".into();
    held.ci_snapshot_age_ms = Some(4_200_000);
    held.runner_id = "xbabe1".into();
    held.runner_epoch = 8;
    held.heartbeat_healthy = true;
    held.startup_rebased = false;
    held.startup_main_ref = Some("origin/main".into());
    held.startup_base_sha = Some("abc123".into());
    held.startup_head_sha = Some("def456".into());
    held.failed_run_id = Some("ci-18".into());
    held.failed_receipt_id = Some("receipt-18".into());
    held.allowed_paths = vec!["/workspace/core/api".into()];
    held.failure_log_digest = Some("sha256:deadbeef".into());

    let mut blocked = WorkcellItem::new("wc-19", "agent-frost-01 / core/docs");
    blocked.claim_state = WorkcellState::Blocked;
    blocked.agent_id = "agent-frost-01".into();
    blocked.repo_roots = vec!["/workspace/core/docs".into()];
    blocked.workspace_root = "/workspace/core/docs".into();
    blocked.branch_budget = 2;
    blocked.branches_open = 1;
    blocked.git_status_summary = "rebase failed after main advanced".into();
    blocked.ci_snapshot_age_ms = Some(4_200_000);
    blocked.runner_id = "xbabe2".into();
    blocked.runner_epoch = 9;
    blocked.heartbeat_healthy = false;
    blocked.startup_rebased = false;
    blocked.startup_main_ref = Some("origin/main".into());
    blocked.startup_base_sha = Some("abc123".into());
    blocked.startup_head_sha = Some("def456".into());

    WorkcellsDashboard {
        items: vec![claimed, held, blocked],
        freshness: Some(SourceFreshness::live(SourceKind::Autonomy, at, "cursor-1")),
        summary: Some(WorkcellsSummary {
            total_workcells: 3,
            warming_workcells: 0,
            ready_workcells: 0,
            claimed_workcells: 1,
            held_workcells: 1,
            repairing_workcells: 0,
            blocked_workcells: 1,
            heartbeat_healthy: 2,
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
