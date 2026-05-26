//! Owner: Interactive TUI subsystem — Jankurai score scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::jankurai`
//! Invariants: Deterministic; pure data; no I/O. Encodes repo-score drift /
//! regression scenarios used by the Jankurai lens.

use crate::api::entity::{ActionRef, BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{AttentionItem, MissionSnapshot, TuiReadModel};
use crate::tui::action_registry::RiskTier;

use super::{fresh, healthy_system, ts};

/// Jankurai regression: repo score dropped vs last baseline.
pub fn jankurai_regression() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(21, 0, 0);
    m.event_cursor = 1_710;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let project_entity = EntityRef::new(EntityKind::Project, "jeryu/main");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "jankurai_drop".into(),
            severity: Severity::Warning,
            summary: "Repo score 84/100 (was 92/100); test-map drift +8".into(),
            entity: Some(project_entity.clone()),
            recommended_action: Some(ActionRef {
                action_id: "plan_validation".into(),
                label: "Plan validation pass".into(),
                risk: Some(RiskTier::R1),
            }),
        }),
        active_agents: 3,
        blocked_agents: 0,
        running_jobs: 4,
        failed_jobs: 0,
        queued_jobs: 2,
        open_capsules: 6,
        active_grants: 2,
        cache_hit_ratio: 0.91,
        active_taints: 0,
        selector_misses_24h: 6,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 29,
        taint_count: 0,
    };
    m.attention.push(AttentionItem {
        id: "att-janku-drop".into(),
        severity: Severity::Warning,
        title: "Jankurai score dropped".into(),
        why_it_matters: "Test-map/owner-map drift; merge gate downgraded.".into(),
        entity: project_entity,
        evidence: vec!["capsule:cap-score-2026-05-26".into()],
        recommended_actions: vec![ActionRef {
            action_id: "plan_validation".into(),
            label: "Plan validation pass".into(),
            risk: Some(RiskTier::R1),
        }],
        created_at: ts(20, 50, 0),
        last_seen_at: ts(21, 0, 0),
    });
    m.system = healthy_system(5, 7);
    m
}

/// Jankurai clean: score at or above baseline; no drift.
pub fn jankurai_clean() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(21, 5, 0);
    m.event_cursor = 1_725;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: true,
        top_blocker: None,
        active_agents: 2,
        blocked_agents: 0,
        running_jobs: 3,
        failed_jobs: 0,
        queued_jobs: 0,
        open_capsules: 3,
        active_grants: 2,
        cache_hit_ratio: 0.95,
        active_taints: 0,
        selector_misses_24h: 0,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 18,
        taint_count: 0,
    };
    m.system = healthy_system(5, 7);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dets(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn jankurai_regression_is_deterministic() {
        assert_eq!(dets(&jankurai_regression()), dets(&jankurai_regression()));
    }

    #[test]
    fn jankurai_clean_is_deterministic() {
        assert_eq!(dets(&jankurai_clean()), dets(&jankurai_clean()));
    }

    #[test]
    fn jankurai_regression_blocks_merge() {
        let m = jankurai_regression();
        assert!(!m.mission.safe_to_merge);
        let b = m.mission.top_blocker.as_ref().unwrap();
        assert_eq!(b.kind, "jankurai_drop");
    }

    #[test]
    fn jankurai_clean_is_healthy() {
        let m = jankurai_clean();
        assert_eq!(m.mission.overall, HealthLevel::Healthy);
    }
}
