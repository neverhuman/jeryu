//! Owner: Interactive TUI subsystem — mission scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::mission`
//! Invariants: Deterministic; pure data; no I/O.

use crate::api::entity::{BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel,
};
use crate::tui::action_registry::RiskTier;

use super::{action, degraded_component, fresh, healthy_system, ts};

/// Healthy fleet: agents can code, gates green, no blockers.
pub fn healthy() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(12, 0, 0);
    m.event_cursor = 100;
    m.freshness = fresh(1_000, 800, 1_200, 900, 1_500, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: true,
        active_agents: 4,
        running_jobs: 12,
        queued_jobs: 3,
        open_capsules: 8,
        active_grants: 5,
        cache_hit_ratio: 0.94,
        selector_misses_24h: 2,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 42,
        ..MissionSnapshot::default()
    };
    m.next_action = Some(NextActionRecommendation {
        action_ref: action("next_action", "Continue", RiskTier::ReadOnly),
        label: "Continue normal operations".into(),
        why: "Fleet healthy; no blockers.".into(),
        entity: None,
        confidence: 0.98,
        safety: ActionSafety::Safe,
        risk: RiskTier::ReadOnly,
    });
    m.system = healthy_system(8, 4);
    m
}

/// Degraded fleet: agents can still code but merges are blocked.
pub fn degraded() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(12, 5, 0);
    m.event_cursor = 250;
    m.freshness = fresh(2_500, 1_100, 1_800, 1_400, 1_700, false);
    let pipe = EntityRef::new(EntityKind::Pipeline, "42137");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Degraded,
        top_blocker: Some(BlockerSummary {
            kind: "pipeline_failure".into(),
            severity: Severity::Error,
            summary: "main pipeline 42137 failed on cache_warm stage".into(),
            entity: Some(pipe.clone()),
            recommended_action: Some(action("open_logs", "Open logs", RiskTier::ReadOnly)),
        }),
        active_agents: 3,
        blocked_agents: 1,
        running_jobs: 7,
        failed_jobs: 2,
        queued_jobs: 11,
        open_capsules: 4,
        active_grants: 3,
        cache_hit_ratio: 0.62,
        active_taints: 1,
        selector_misses_24h: 18,
        active_runners: 9,
        total_runners: 12,
        evidence_count: 31,
        taint_count: 1,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-pipe-42137".into(),
        severity: Severity::Error,
        title: "Pipeline 42137 failed".into(),
        why_it_matters: "Merge gate red; downstream releases blocked.".into(),
        entity: pipe.clone(),
        evidence: vec!["capsule:cap-42137-cache".into()],
        recommended_actions: vec![action("open_logs", "Open logs", RiskTier::ReadOnly)],
        created_at: ts(11, 55, 0),
        last_seen_at: ts(12, 4, 30),
    });
    m.next_action = Some(NextActionRecommendation {
        action_ref: action("open_logs", "Open logs", RiskTier::ReadOnly),
        label: "Inspect failing pipeline".into(),
        why: "Pipeline 42137 cache_warm stage failed; root cause unknown.".into(),
        entity: Some(pipe),
        confidence: 0.82,
        safety: ActionSafety::Safe,
        risk: RiskTier::ReadOnly,
    });
    let mut sys = healthy_system(6, 3);
    sys.cache = degraded_component("cache", "hit ratio 62%", 45);
    sys.runners.online = 9;
    sys.runners.degraded = 3;
    m.system = sys;
    m
}

/// Incident in progress: prod-impacting; safe_to_code = false.
pub fn incident() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(12, 11, 17);
    m.event_cursor = 800;
    m.freshness = fresh(800, 600, 1_000, 700, 900, false);
    let rel = EntityRef::new(EntityKind::ReleaseAttempt, "rel-12.0.0");
    let explain = action("explain_blockers", "Explain blockers", RiskTier::ReadOnly);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Critical,
        safe_to_code: false,
        top_blocker: Some(BlockerSummary {
            kind: "incident".into(),
            severity: Severity::Critical,
            summary: "INC-2026-05-26-001: prod canary 12.0.0 rolling back".into(),
            entity: Some(rel.clone()),
            recommended_action: Some(explain.clone()),
        }),
        blocked_agents: 6,
        running_jobs: 1,
        failed_jobs: 9,
        open_capsules: 22,
        cache_hit_ratio: 0.31,
        active_taints: 4,
        selector_misses_24h: 64,
        agents_can_code: false,
        active_runners: 4,
        total_runners: 12,
        evidence_count: 90,
        taint_count: 4,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-inc-001".into(),
        severity: Severity::Critical,
        title: "INC-2026-05-26-001 declared".into(),
        why_it_matters: "Canary rollout 12.0.0 failed on prod; rollback in progress.".into(),
        entity: rel.clone(),
        evidence: vec!["capsule:cap-inc-001".into()],
        recommended_actions: vec![explain.clone()],
        created_at: ts(12, 9, 0),
        last_seen_at: ts(12, 11, 0),
    });
    m.next_action = Some(NextActionRecommendation {
        action_ref: explain,
        label: "Read incident blocker chain".into(),
        why: "Code freeze active; understand blocker chain before acting.".into(),
        entity: Some(rel),
        confidence: 0.95,
        safety: ActionSafety::Safe,
        risk: RiskTier::ReadOnly,
    });
    let mut sys = healthy_system(1, 3);
    sys.cache = degraded_component("cache", "miss rate 69%", 45);
    sys.runners.online = 4;
    sys.runners.degraded = 8;
    m.system = sys;
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn all_scenarios_are_deterministic() {
        assert_eq!(det(&healthy()), det(&healthy()));
        assert_eq!(det(&degraded()), det(&degraded()));
        assert_eq!(det(&incident()), det(&incident()));
    }

    #[test]
    fn gate_states_match_scenario() {
        let h = healthy();
        assert!(h.mission.safe_to_code && h.mission.safe_to_merge);
        assert!(h.mission.top_blocker.is_none());
        let d = degraded();
        assert!(d.mission.safe_to_code && !d.mission.safe_to_merge);
        assert!(d.mission.top_blocker.is_some());
        let i = incident();
        assert!(!i.mission.safe_to_code && !i.mission.agents_can_code);
        assert_eq!(i.mission.overall, HealthLevel::Critical);
    }
}
