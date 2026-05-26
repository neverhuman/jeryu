//! Owner: Interactive TUI subsystem — incident scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::incident`
//! Invariants: Deterministic; pure data; no I/O. Encodes pinned-incident
//! and recovered-incident states used by the incident overlay.

use crate::api::entity::{ActionRef, BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel,
};
use crate::tui::action_registry::RiskTier;

use super::{degraded_component, fresh, healthy_system, ts};

/// Incident pinned: an open incident requires acknowledged investigation.
pub fn incident_pinned() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(22, 0, 0);
    m.event_cursor = 1_810;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let inc_entity = EntityRef::new(EntityKind::System, "INC-2026-05-26-002");
    let blocker = BlockerSummary {
        kind: "incident_open".into(),
        severity: Severity::Critical,
        summary: "INC-2026-05-26-002: GitLab API 5xx burst; agent throughput halved".into(),
        entity: Some(inc_entity.clone()),
        recommended_action: Some(ActionRef {
            action_id: "fetch_capsule".into(),
            label: "Fetch incident capsule".into(),
            risk: Some(RiskTier::ReadOnly),
        }),
    };
    m.mission = MissionSnapshot {
        overall: HealthLevel::Critical,
        safe_to_code: false,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(blocker),
        active_agents: 1,
        blocked_agents: 4,
        running_jobs: 1,
        failed_jobs: 5,
        queued_jobs: 12,
        open_capsules: 17,
        active_grants: 1,
        cache_hit_ratio: 0.74,
        active_taints: 2,
        selector_misses_24h: 31,
        agents_can_code: false,
        active_runners: 9,
        total_runners: 12,
        evidence_count: 88,
        taint_count: 2,
    };
    m.attention.push(AttentionItem {
        id: "att-inc-002".into(),
        severity: Severity::Critical,
        title: "INC-2026-05-26-002 open".into(),
        why_it_matters: "GitLab API errors; investigations require fresh data.".into(),
        entity: inc_entity.clone(),
        evidence: vec!["capsule:cap-inc-002-pinpoint".into()],
        recommended_actions: vec![ActionRef {
            action_id: "fetch_capsule".into(),
            label: "Fetch incident capsule".into(),
            risk: Some(RiskTier::ReadOnly),
        }],
        created_at: ts(21, 50, 0),
        last_seen_at: ts(22, 0, 0),
    });
    m.next_action = Some(NextActionRecommendation {
        action_ref: ActionRef {
            action_id: "fetch_capsule".into(),
            label: "Fetch incident capsule".into(),
            risk: Some(RiskTier::ReadOnly),
        },
        label: "Pull incident evidence".into(),
        why: "Incident open; evidence required before declaring resolved.".into(),
        entity: Some(inc_entity),
        confidence: 0.96,
        safety: ActionSafety::Safe,
        risk: RiskTier::ReadOnly,
    });
    let mut sys = healthy_system(4, 5);
    sys.gitlab = degraded_component("gitlab", "5xx burst", 120);
    sys.runners.online = 9;
    sys.runners.degraded = 3;
    m.system = sys;
    m
}

/// Incident recovered: the postmortem capsule is the only attention item.
pub fn incident_recovered() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(22, 30, 0);
    m.event_cursor = 1_920;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let inc_entity = EntityRef::new(EntityKind::System, "INC-2026-05-26-002");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: false,
        top_blocker: None,
        active_agents: 3,
        blocked_agents: 0,
        running_jobs: 4,
        failed_jobs: 0,
        queued_jobs: 1,
        open_capsules: 9,
        active_grants: 2,
        cache_hit_ratio: 0.92,
        active_taints: 0,
        selector_misses_24h: 8,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 95,
        taint_count: 0,
    };
    m.attention.push(AttentionItem {
        id: "att-inc-002-pm".into(),
        severity: Severity::Info,
        title: "INC-2026-05-26-002 postmortem pending".into(),
        why_it_matters: "Service recovered; postmortem capsule awaiting review.".into(),
        entity: inc_entity,
        evidence: vec!["capsule:cap-inc-002-postmortem".into()],
        recommended_actions: vec![ActionRef {
            action_id: "fetch_capsule".into(),
            label: "Read postmortem".into(),
            risk: Some(RiskTier::ReadOnly),
        }],
        created_at: ts(22, 25, 0),
        last_seen_at: ts(22, 30, 0),
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
    fn incident_pinned_is_deterministic() {
        assert_eq!(dets(&incident_pinned()), dets(&incident_pinned()));
    }

    #[test]
    fn incident_recovered_is_deterministic() {
        assert_eq!(dets(&incident_recovered()), dets(&incident_recovered()));
    }

    #[test]
    fn incident_pinned_freezes_code() {
        let m = incident_pinned();
        assert!(!m.mission.safe_to_code);
        assert_eq!(m.mission.overall, HealthLevel::Critical);
        assert!(m.mission.top_blocker.is_some());
    }

    #[test]
    fn incident_recovered_thaws_code() {
        let m = incident_recovered();
        assert!(m.mission.safe_to_code);
        assert!(m.mission.safe_to_merge);
        assert!(m.mission.top_blocker.is_none());
    }
}
