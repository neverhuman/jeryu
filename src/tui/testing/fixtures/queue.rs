//! Owner: Interactive TUI subsystem — queue scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::queue`
//! Invariants: Deterministic; pure data; no I/O. Encodes queue-depth and
//! runner-saturation scenarios used by the Queue / theoretical-limit lens.

use crate::api::entity::{BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel,
};
use crate::tui::action_registry::RiskTier;

use super::{action, fresh, healthy_system, ts};

/// Queue saturated: more queued than runners; adding runners would help.
pub fn saturated() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(13, 0, 0);
    m.event_cursor = 320;
    m.freshness = fresh(900, 700, 1_100, 800, 1_300, false);
    let pool = EntityRef::new(EntityKind::Pool, "pool-default");
    let pause = action("pause_pool", "Pause non-critical pool", RiskTier::Low);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "queue_saturation".into(),
            severity: Severity::Warning,
            summary: "144 queued jobs against 12 runners (12x oversubscribed)".into(),
            entity: Some(pool.clone()),
            recommended_action: Some(pause.clone()),
        }),
        active_agents: 4,
        running_jobs: 12,
        queued_jobs: 144,
        open_capsules: 6,
        active_grants: 4,
        cache_hit_ratio: 0.88,
        selector_misses_24h: 5,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 28,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-queue-sat".into(),
        severity: Severity::Warning,
        title: "Queue saturated".into(),
        why_it_matters: "All runners busy; new jobs wait avg 11m.".into(),
        entity: pool.clone(),
        evidence: vec![],
        recommended_actions: vec![pause.clone()],
        created_at: ts(12, 55, 0),
        last_seen_at: ts(13, 0, 0),
    });
    m.next_action = Some(NextActionRecommendation {
        action_ref: pause,
        label: "Pause non-critical pool to relieve queue".into(),
        why: "Queue saturated; pausing low-priority pool frees runners.".into(),
        entity: Some(pool),
        confidence: 0.74,
        safety: ActionSafety::Reversible,
        risk: RiskTier::Low,
    });
    m.system = healthy_system(12, 0);
    m
}

/// Queue idle: no pending work, runners largely idle.
pub fn idle() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(13, 5, 0);
    m.event_cursor = 330;
    m.freshness = fresh(1_100, 900, 1_300, 1_000, 1_400, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: true,
        active_grants: 1,
        cache_hit_ratio: 0.97,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 12,
        open_capsules: 2,
        ..MissionSnapshot::default()
    };
    m.system = healthy_system(0, 12);
    m
}

/// Queue in a serial DAG: linear chain prevents parallelism even with capacity.
/// More runners would NOT help — bottleneck is the critical path.
pub fn serial_dag() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(13, 10, 0);
    m.event_cursor = 410;
    m.freshness = fresh(1_000, 800, 1_200, 900, 1_500, false);
    let pipe = EntityRef::new(EntityKind::Pipeline, "p-serial");
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "serial_dag".into(),
            severity: Severity::Warning,
            summary: "Critical path is serial (8 jobs); adding runners will NOT help".into(),
            entity: Some(pipe.clone()),
            recommended_action: None,
        }),
        active_agents: 2,
        running_jobs: 1,
        queued_jobs: 7,
        open_capsules: 4,
        active_grants: 2,
        cache_hit_ratio: 0.91,
        selector_misses_24h: 1,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 19,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-serial".into(),
        severity: Severity::Warning,
        title: "Serial DAG bottleneck".into(),
        why_it_matters: "Critical path is linear; parallelism cannot reduce latency.".into(),
        entity: pipe,
        evidence: vec!["capsule:cap-dag-shape".into()],
        recommended_actions: vec![],
        created_at: ts(13, 7, 0),
        last_seen_at: ts(13, 10, 0),
    });
    m.system = healthy_system(1, 11);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dets(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn saturated_is_deterministic() {
        assert_eq!(dets(&saturated()), dets(&saturated()));
    }

    #[test]
    fn idle_is_deterministic() {
        assert_eq!(dets(&idle()), dets(&idle()));
    }

    #[test]
    fn serial_dag_is_deterministic() {
        assert_eq!(dets(&serial_dag()), dets(&serial_dag()));
    }

    #[test]
    fn saturated_has_many_queued() {
        let m = saturated();
        assert!(m.mission.queued_jobs > m.mission.total_runners * 4);
        assert_eq!(m.system.runners.idle, 0);
    }

    #[test]
    fn idle_has_no_work() {
        let m = idle();
        assert_eq!(m.mission.queued_jobs, 0);
        assert_eq!(m.mission.running_jobs, 0);
    }

    #[test]
    fn serial_dag_reports_serial_path() {
        let m = serial_dag();
        let b = m.mission.top_blocker.as_ref().unwrap();
        assert_eq!(b.kind, "serial_dag");
    }
}
