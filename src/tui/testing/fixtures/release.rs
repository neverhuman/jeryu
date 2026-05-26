//! Owner: Interactive TUI subsystem — release scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::release`
//! Invariants: Deterministic; pure data; no I/O. Encodes release-gate
//! scenarios used by the Release / canary lens.

use crate::api::entity::{BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel,
};
use crate::tui::action_registry::RiskTier;

use super::{action, fresh, healthy_system, ts};

/// Release blocked at a gate: gate not satisfied, safe_to_release = false.
pub fn release_blocked() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(15, 0, 0);
    m.event_cursor = 800;
    m.freshness = fresh(900, 700, 1_000, 800, 1_200, false);
    let gate = EntityRef::new(EntityKind::ReleaseGate, "gate-security-scan");
    let explain = action("explain_blockers", "Explain blockers", RiskTier::R0);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Degraded,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "release_gate".into(),
            severity: Severity::Error,
            summary: "Security scan gate failed: 2 high CVEs in transitive deps".into(),
            entity: Some(gate.clone()),
            recommended_action: Some(explain.clone()),
        }),
        active_agents: 2,
        open_capsules: 6,
        active_grants: 3,
        cache_hit_ratio: 0.94,
        selector_misses_24h: 2,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 41,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-gate-security".into(),
        severity: Severity::Error,
        title: "Security gate blocking release".into(),
        why_it_matters: "Two high-severity CVEs detected; cannot ship.".into(),
        entity: gate,
        evidence: vec!["capsule:cap-cve-scan".into()],
        recommended_actions: vec![explain],
        created_at: ts(14, 58, 0),
        last_seen_at: ts(15, 0, 0),
    });
    m.system = healthy_system(4, 8);
    m
}

/// Canary running: release attempt mid-flight, watching for regressions.
pub fn canary_running() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(15, 5, 0);
    m.event_cursor = 905;
    m.freshness = fresh(700, 600, 800, 600, 1_000, false);
    let rel = EntityRef::new(EntityKind::ReleaseAttempt, "rel-12.0.0-canary");
    let fetch = action("fetch_capsule", "Fetch canary capsule", RiskTier::R0);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: false,
        safe_to_merge: false,
        safe_to_release: false,
        active_agents: 1,
        running_jobs: 2,
        open_capsules: 9,
        active_grants: 4,
        cache_hit_ratio: 0.95,
        agents_can_code: false,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 58,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-canary".into(),
        severity: Severity::Warning,
        title: "Canary 12.0.0 in flight".into(),
        why_it_matters: "Code freeze active until canary observation window closes.".into(),
        entity: rel.clone(),
        evidence: vec!["capsule:cap-canary-baseline".into()],
        recommended_actions: vec![fetch.clone()],
        created_at: ts(14, 50, 0),
        last_seen_at: ts(15, 5, 0),
    });
    m.next_action = Some(NextActionRecommendation {
        action_ref: fetch,
        label: "Read canary metrics capsule".into(),
        why: "Canary observation window open; metrics inform go/no-go.".into(),
        entity: Some(rel),
        confidence: 0.79,
        safety: ActionSafety::Safe,
        risk: RiskTier::R0,
    });
    m.system = healthy_system(4, 8);
    m
}

/// Rollback prepared: stable previous artifact ready, awaiting decision.
pub fn rollback_ready() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(15, 10, 0);
    m.event_cursor = 1_050;
    m.freshness = fresh(800, 600, 900, 700, 1_100, false);
    let rel = EntityRef::new(EntityKind::ReleaseAttempt, "rel-12.0.0-rollback");
    let explain = action("explain_blockers", "Explain blockers", RiskTier::R0);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Critical,
        safe_to_code: false,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "rollback_required".into(),
            severity: Severity::Critical,
            summary: "Canary 12.0.0 regression confirmed; rollback ready.".into(),
            entity: Some(rel.clone()),
            recommended_action: Some(explain.clone()),
        }),
        blocked_agents: 4,
        failed_jobs: 3,
        open_capsules: 14,
        active_grants: 1,
        cache_hit_ratio: 0.82,
        active_taints: 1,
        selector_misses_24h: 9,
        agents_can_code: false,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 73,
        taint_count: 1,
        ..MissionSnapshot::default()
    };
    m.next_action = Some(NextActionRecommendation {
        action_ref: explain,
        label: "Review rollback evidence before approval".into(),
        why: "Production-impact action; need full evidence chain.".into(),
        entity: Some(rel),
        confidence: 0.93,
        safety: ActionSafety::Safe,
        risk: RiskTier::R0,
    });
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
    fn release_blocked_is_deterministic() {
        assert_eq!(dets(&release_blocked()), dets(&release_blocked()));
    }

    #[test]
    fn canary_running_is_deterministic() {
        assert_eq!(dets(&canary_running()), dets(&canary_running()));
    }

    #[test]
    fn rollback_ready_is_deterministic() {
        assert_eq!(dets(&rollback_ready()), dets(&rollback_ready()));
    }

    #[test]
    fn release_blocked_forbids_ship() {
        let m = release_blocked();
        assert!(!m.mission.safe_to_release);
        assert!(m.mission.top_blocker.is_some());
    }

    #[test]
    fn canary_running_freezes_code() {
        let m = canary_running();
        assert!(!m.mission.safe_to_code);
        assert_eq!(m.mission.overall, HealthLevel::Warning);
    }

    #[test]
    fn rollback_ready_is_critical() {
        let m = rollback_ready();
        assert_eq!(m.mission.overall, HealthLevel::Critical);
    }
}
