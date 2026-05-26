//! Owner: Interactive TUI subsystem — agent scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::agents`
//! Invariants: Deterministic; pure data; no I/O. Encodes agent race / idle
//! scenarios used by the Agents lens.

use crate::api::entity::{ActionRef, BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{AttentionItem, MissionSnapshot, TuiReadModel};
use crate::tui::action_registry::RiskTier;

use super::{fresh, healthy_system, ts};

/// Agent race: two patch-racers competing on the same bug.
pub fn agent_race() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(18, 0, 0);
    m.event_cursor = 1_410;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let agent_a = EntityRef::new(EntityKind::Agent, "wrath-17");
    let agent_b = EntityRef::new(EntityKind::Agent, "envy-04");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "agent_race".into(),
            severity: Severity::Warning,
            summary: "Agents wrath-17 and envy-04 both racing patch on bug BUG-42".into(),
            entity: Some(agent_a.clone()),
            recommended_action: None,
        }),
        active_agents: 4,
        blocked_agents: 0,
        running_jobs: 6,
        failed_jobs: 0,
        queued_jobs: 1,
        open_capsules: 5,
        active_grants: 4,
        cache_hit_ratio: 0.92,
        active_taints: 0,
        selector_misses_24h: 2,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 31,
        taint_count: 0,
    };
    m.attention.push(AttentionItem {
        id: "att-race-a".into(),
        severity: Severity::Warning,
        title: "wrath-17 racing BUG-42".into(),
        why_it_matters: "Multiple agents on same bug; outputs will be raced.".into(),
        entity: agent_a,
        evidence: vec!["capsule:cap-grant-wrath-17".into()],
        recommended_actions: vec![ActionRef {
            action_id: "race_patches".into(),
            label: "Race patches".into(),
            risk: Some(RiskTier::R3),
        }],
        created_at: ts(17, 55, 0),
        last_seen_at: ts(18, 0, 0),
    });
    m.attention.push(AttentionItem {
        id: "att-race-b".into(),
        severity: Severity::Info,
        title: "envy-04 racing BUG-42".into(),
        why_it_matters: "Multiple agents on same bug; outputs will be raced.".into(),
        entity: agent_b,
        evidence: vec!["capsule:cap-grant-envy-04".into()],
        recommended_actions: vec![],
        created_at: ts(17, 56, 0),
        last_seen_at: ts(18, 0, 0),
    });
    m.system = healthy_system(4, 8);
    m
}

/// Agents idle: no active tasks, no grants in play.
pub fn agent_idle() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(18, 5, 0);
    m.event_cursor = 1_420;
    m.freshness = fresh(800, 600, 1_000, 700, 1_200, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: true,
        top_blocker: None,
        active_agents: 0,
        blocked_agents: 0,
        running_jobs: 0,
        failed_jobs: 0,
        queued_jobs: 0,
        open_capsules: 2,
        active_grants: 0,
        cache_hit_ratio: 0.95,
        active_taints: 0,
        selector_misses_24h: 0,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 10,
        taint_count: 0,
    };
    m.system = healthy_system(4, 8);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dets(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn agent_race_is_deterministic() {
        assert_eq!(dets(&agent_race()), dets(&agent_race()));
    }

    #[test]
    fn agent_idle_is_deterministic() {
        assert_eq!(dets(&agent_idle()), dets(&agent_idle()));
    }

    #[test]
    fn agent_race_shows_two_attention() {
        let m = agent_race();
        assert!(m.attention.len() >= 2);
        assert!(m.mission.active_agents >= 2);
    }

    #[test]
    fn agent_idle_has_no_agents() {
        let m = agent_idle();
        assert_eq!(m.mission.active_agents, 0);
        assert_eq!(m.mission.active_grants, 0);
    }
}
