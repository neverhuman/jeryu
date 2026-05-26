//! Owner: Interactive TUI subsystem — workflow scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::workflow`
//! Invariants: Deterministic; pure data; no I/O. Encodes pipeline / MR
//! workflow scenarios used by the Workflow atlas lens.

use crate::api::entity::{BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel,
};
use crate::tui::action_registry::RiskTier;

use super::{action, fresh, healthy_system, ts};

/// Single MR with a single pipeline currently running.
pub fn single_pipeline() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(14, 0, 0);
    m.event_cursor = 510;
    m.freshness = fresh(1_000, 700, 1_100, 900, 1_300, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        active_agents: 2,
        running_jobs: 4,
        open_capsules: 3,
        active_grants: 1,
        cache_hit_ratio: 0.95,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 22,
        ..MissionSnapshot::default()
    };
    m.next_action = Some(NextActionRecommendation {
        action_ref: action("get_pipeline_jobs", "View pipeline jobs", RiskTier::R0),
        label: "Track pipeline 51001 progress".into(),
        why: "Pipeline running; no blockers yet.".into(),
        entity: Some(EntityRef::new(EntityKind::Pipeline, "51001")),
        confidence: 0.86,
        safety: ActionSafety::Safe,
        risk: RiskTier::R0,
    });
    m.system = healthy_system(8, 4);
    m
}

/// Multiple MRs with multiple pipelines simultaneously running.
pub fn multi_pipeline() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(14, 5, 0);
    m.event_cursor = 612;
    m.freshness = fresh(900, 700, 1_100, 800, 1_200, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        active_agents: 5,
        running_jobs: 18,
        queued_jobs: 4,
        open_capsules: 7,
        active_grants: 5,
        cache_hit_ratio: 0.93,
        selector_misses_24h: 1,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 51,
        ..MissionSnapshot::default()
    };
    for (idx, pid) in ["52010", "52011", "52012", "52013"].iter().enumerate() {
        m.attention.push(AttentionItem {
            id: format!("att-pipe-{pid}"),
            severity: Severity::Info,
            title: format!("Pipeline {pid} running"),
            why_it_matters: "Multi-MR fan-out; track parallel pipelines.".into(),
            entity: EntityRef::new(EntityKind::Pipeline, *pid),
            evidence: vec![],
            recommended_actions: vec![],
            created_at: ts(14, 0, idx as u32),
            last_seen_at: ts(14, 5, idx as u32),
        });
    }
    m.system = healthy_system(8, 4);
    m
}

/// Failed pipeline: merge blocked; needs investigation.
pub fn failed() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(14, 10, 0);
    m.event_cursor = 720;
    m.freshness = fresh(1_000, 800, 1_200, 900, 1_400, false);
    let pipe = EntityRef::new(EntityKind::Pipeline, "52020");
    let open_logs = action("open_logs", "Open logs", RiskTier::R0);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Degraded,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "pipeline_failure".into(),
            severity: Severity::Error,
            summary: "Pipeline 52020 failed on test_e2e".into(),
            entity: Some(pipe.clone()),
            recommended_action: Some(open_logs.clone()),
        }),
        active_agents: 2,
        blocked_agents: 1,
        failed_jobs: 1,
        open_capsules: 5,
        active_grants: 2,
        cache_hit_ratio: 0.81,
        selector_misses_24h: 4,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 33,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-pipe-52020".into(),
        severity: Severity::Error,
        title: "Pipeline 52020 failed".into(),
        why_it_matters: "Blocks MR !1138 merge; e2e regression suspected.".into(),
        entity: pipe.clone(),
        evidence: vec!["capsule:cap-52020-e2e".into()],
        recommended_actions: vec![open_logs.clone()],
        created_at: ts(14, 8, 0),
        last_seen_at: ts(14, 10, 0),
    });
    m.next_action = Some(NextActionRecommendation {
        action_ref: open_logs,
        label: "Open failing job logs".into(),
        why: "test_e2e failed; logs hold the failure stack.".into(),
        entity: Some(pipe),
        confidence: 0.91,
        safety: ActionSafety::Safe,
        risk: RiskTier::R0,
    });
    m.system = healthy_system(8, 4);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dets(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn single_pipeline_is_deterministic() {
        assert_eq!(dets(&single_pipeline()), dets(&single_pipeline()));
    }

    #[test]
    fn multi_pipeline_is_deterministic() {
        assert_eq!(dets(&multi_pipeline()), dets(&multi_pipeline()));
    }

    #[test]
    fn failed_is_deterministic() {
        assert_eq!(dets(&failed()), dets(&failed()));
    }

    #[test]
    fn single_pipeline_has_no_blocker() {
        let m = single_pipeline();
        assert!(m.mission.top_blocker.is_none());
    }

    #[test]
    fn multi_pipeline_has_multiple_attention_items() {
        let m = multi_pipeline();
        assert!(m.attention.len() >= 4);
    }

    #[test]
    fn failed_blocks_merge() {
        let m = failed();
        assert!(!m.mission.safe_to_merge);
        assert_eq!(m.mission.failed_jobs, 1);
    }
}
