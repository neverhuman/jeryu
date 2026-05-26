//! Owner: Interactive TUI subsystem — bug scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::bugs`
//! Invariants: Deterministic; pure data; no I/O. Encodes bug-queue states
//! used by the Bugs lens.

use crate::api::entity::{ActionRef, BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{AttentionItem, MissionSnapshot, TuiReadModel};
use crate::tui::action_registry::RiskTier;

use super::{fresh, healthy_system, ts};

/// Bug ready: triaged, patchable, awaiting agent assignment.
pub fn bug_ready() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(19, 0, 0);
    m.event_cursor = 1_510;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let bug_entity = EntityRef::new(EntityKind::Bug, "BUG-101");
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
        open_capsules: 5,
        active_grants: 2,
        cache_hit_ratio: 0.93,
        active_taints: 0,
        selector_misses_24h: 1,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 26,
        taint_count: 0,
    };
    m.attention.push(AttentionItem {
        id: "att-bug-101".into(),
        severity: Severity::Info,
        title: "BUG-101 ready for patching".into(),
        why_it_matters: "Triaged, scoped, has repro; assign for next race.".into(),
        entity: bug_entity,
        evidence: vec!["capsule:cap-bug-101-repro".into()],
        recommended_actions: vec![ActionRef {
            action_id: "bug_ready".into(),
            label: "Mark ready".into(),
            risk: Some(RiskTier::Low),
        }],
        created_at: ts(18, 55, 0),
        last_seen_at: ts(19, 0, 0),
    });
    m.system = healthy_system(5, 7);
    m
}

/// Bug blocked: cannot proceed without external decision or info.
pub fn bug_blocked() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(19, 5, 0);
    m.event_cursor = 1_525;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let bug_entity = EntityRef::new(EntityKind::Bug, "BUG-102");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "bug_blocked".into(),
            severity: Severity::Warning,
            summary: "BUG-102 blocked: missing repro from reporter".into(),
            entity: Some(bug_entity.clone()),
            recommended_action: None,
        }),
        active_agents: 1,
        blocked_agents: 1,
        running_jobs: 0,
        failed_jobs: 0,
        queued_jobs: 0,
        open_capsules: 6,
        active_grants: 1,
        cache_hit_ratio: 0.93,
        active_taints: 0,
        selector_misses_24h: 3,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 21,
        taint_count: 0,
    };
    m.attention.push(AttentionItem {
        id: "att-bug-102".into(),
        severity: Severity::Warning,
        title: "BUG-102 blocked".into(),
        why_it_matters: "Awaiting reporter info; agent grant idling.".into(),
        entity: bug_entity,
        evidence: vec!["capsule:cap-bug-102-blocker".into()],
        recommended_actions: vec![ActionRef {
            action_id: "bug_update".into(),
            label: "Update blocker note".into(),
            risk: Some(RiskTier::Low),
        }],
        created_at: ts(18, 30, 0),
        last_seen_at: ts(19, 5, 0),
    });
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
    fn bug_ready_is_deterministic() {
        assert_eq!(dets(&bug_ready()), dets(&bug_ready()));
    }

    #[test]
    fn bug_blocked_is_deterministic() {
        assert_eq!(dets(&bug_blocked()), dets(&bug_blocked()));
    }

    #[test]
    fn bug_ready_is_healthy() {
        let m = bug_ready();
        assert_eq!(m.mission.overall, HealthLevel::Healthy);
        assert!(m.mission.top_blocker.is_none());
    }

    #[test]
    fn bug_blocked_has_blocker() {
        let m = bug_blocked();
        let b = m.mission.top_blocker.as_ref().unwrap();
        assert_eq!(b.kind, "bug_blocked");
    }
}
