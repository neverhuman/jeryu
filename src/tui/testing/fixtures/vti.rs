//! Owner: Interactive TUI subsystem — VTI (validated-test-impact) fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::vti`
//! Invariants: Deterministic; pure data; no I/O. VTI low-confidence must
//! never render as safe — `safe_to_merge=false` on miss.

use crate::api::entity::{ActionRef, BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{AttentionItem, MissionSnapshot, TuiReadModel};
use crate::tui::action_registry::RiskTier;

use super::{fresh, healthy_system, ts};

/// VTI miss: low confidence — cannot vouch that change is covered.
pub fn vti_miss() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(17, 0, 0);
    m.event_cursor = 1_310;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let plan_entity = EntityRef::new(EntityKind::TestPlan, "tp-vti-2026-05-26");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "vti_low_confidence".into(),
            severity: Severity::Warning,
            summary: "VTI confidence 0.42 — change not vouched by impacted tests.".into(),
            entity: Some(plan_entity.clone()),
            recommended_action: Some(ActionRef {
                action_id: "run_tests".into(),
                label: "Run impacted test plan".into(),
                risk: Some(RiskTier::Low),
            }),
        }),
        active_agents: 2,
        blocked_agents: 0,
        running_jobs: 0,
        failed_jobs: 0,
        queued_jobs: 0,
        open_capsules: 3,
        active_grants: 2,
        cache_hit_ratio: 0.91,
        active_taints: 0,
        selector_misses_24h: 8,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 18,
        taint_count: 0,
    };
    m.attention.push(AttentionItem {
        id: "att-vti-miss".into(),
        severity: Severity::Warning,
        title: "VTI low confidence".into(),
        why_it_matters: "Cannot certify change is test-covered; merge unsafe.".into(),
        entity: plan_entity,
        evidence: vec!["capsule:cap-vti-conf-042".into()],
        recommended_actions: vec![ActionRef {
            action_id: "run_tests".into(),
            label: "Run impacted test plan".into(),
            risk: Some(RiskTier::Low),
        }],
        created_at: ts(16, 55, 0),
        last_seen_at: ts(17, 0, 0),
    });
    m.system = healthy_system(6, 6);
    m
}

/// VTI safe: high confidence; impacted tests all covered by the plan.
pub fn vti_safe() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(17, 5, 0);
    m.event_cursor = 1_325;
    m.freshness = fresh(800, 600, 1_000, 700, 1_200, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: true,
        top_blocker: None,
        active_agents: 3,
        blocked_agents: 0,
        running_jobs: 2,
        failed_jobs: 0,
        queued_jobs: 0,
        open_capsules: 4,
        active_grants: 2,
        cache_hit_ratio: 0.96,
        active_taints: 0,
        selector_misses_24h: 0,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 24,
        taint_count: 0,
    };
    m.system = healthy_system(6, 6);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dets(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn vti_miss_is_deterministic() {
        assert_eq!(dets(&vti_miss()), dets(&vti_miss()));
    }

    #[test]
    fn vti_safe_is_deterministic() {
        assert_eq!(dets(&vti_safe()), dets(&vti_safe()));
    }

    #[test]
    fn vti_miss_blocks_merge() {
        let m = vti_miss();
        // INVARIANT: VTI low-confidence must never render as safe to merge.
        assert!(!m.mission.safe_to_merge);
        assert!(m.mission.top_blocker.is_some());
    }

    #[test]
    fn vti_safe_clears_merge() {
        let m = vti_safe();
        assert!(m.mission.safe_to_merge);
        assert!(m.mission.top_blocker.is_none());
    }
}
