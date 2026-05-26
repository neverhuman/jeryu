//! Owner: Interactive TUI subsystem — security scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::security`
//! Invariants: Deterministic; pure data; no I/O. Encodes secret-scan,
//! audit, and clean-bill-of-health states.

use crate::api::entity::{ActionRef, BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{AttentionItem, MissionSnapshot, TuiReadModel};
use crate::tui::action_registry::RiskTier;

use super::{fresh, healthy_system, ts};

/// Security blocked: an active secret leak / unaudited grant is gating work.
pub fn security_blocked() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(20, 0, 0);
    m.event_cursor = 1_610;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let secret_entity = EntityRef::new(EntityKind::SecretAccess, "leak-2026-05-26-001");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Critical,
        safe_to_code: false,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "secret_leak".into(),
            severity: Severity::Critical,
            summary: "Active secret leak detected — credential rotation pending".into(),
            entity: Some(secret_entity.clone()),
            recommended_action: Some(ActionRef {
                action_id: "explain_blockers".into(),
                label: "Explain blockers".into(),
                risk: Some(RiskTier::ReadOnly),
            }),
        }),
        active_agents: 0,
        blocked_agents: 5,
        running_jobs: 0,
        failed_jobs: 1,
        queued_jobs: 0,
        open_capsules: 8,
        active_grants: 0,
        cache_hit_ratio: 0.88,
        active_taints: 0,
        selector_misses_24h: 2,
        agents_can_code: false,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 35,
        taint_count: 0,
    };
    m.attention.push(AttentionItem {
        id: "att-leak-001".into(),
        severity: Severity::Critical,
        title: "Secret leak detected".into(),
        why_it_matters: "All agent grants suspended until rotation complete.".into(),
        entity: secret_entity,
        evidence: vec!["capsule:cap-leak-2026-05-26-001".into()],
        recommended_actions: vec![ActionRef {
            action_id: "explain_blockers".into(),
            label: "Explain blockers".into(),
            risk: Some(RiskTier::ReadOnly),
        }],
        created_at: ts(19, 50, 0),
        last_seen_at: ts(20, 0, 0),
    });
    m.system = healthy_system(4, 8);
    m
}

/// Security clean: no active findings, audit ledger green.
pub fn security_clean() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(20, 5, 0);
    m.event_cursor = 1_625;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
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
        queued_jobs: 1,
        open_capsules: 4,
        active_grants: 2,
        cache_hit_ratio: 0.95,
        active_taints: 0,
        selector_misses_24h: 0,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 22,
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
    fn security_blocked_is_deterministic() {
        assert_eq!(dets(&security_blocked()), dets(&security_blocked()));
    }

    #[test]
    fn security_clean_is_deterministic() {
        assert_eq!(dets(&security_clean()), dets(&security_clean()));
    }

    #[test]
    fn security_blocked_freezes_everything() {
        let m = security_blocked();
        assert!(!m.mission.safe_to_code);
        assert!(!m.mission.safe_to_merge);
        assert!(!m.mission.safe_to_release);
        assert!(!m.mission.agents_can_code);
    }

    #[test]
    fn security_clean_is_healthy() {
        let m = security_clean();
        assert_eq!(m.mission.overall, HealthLevel::Healthy);
        assert!(m.mission.top_blocker.is_none());
    }
}
