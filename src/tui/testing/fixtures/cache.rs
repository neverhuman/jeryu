//! Owner: Interactive TUI subsystem — cache scenario fixtures (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::cache`
//! Invariants: Deterministic; pure data; no I/O. Encodes cache hit/miss,
//! taint storm, and warm-cache scenarios used by the Cache observatory lens.

use crate::api::entity::{BlockerSummary, EntityKind, EntityRef, HealthLevel, Severity};
use crate::api::read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, SystemHealth,
    TuiReadModel,
};
use crate::tui::action_registry::RiskTier;

use super::{action, degraded_component, fresh, healthy_system, ts};

/// Cache pressure: hit ratio below threshold, selector misses climbing.
pub fn cache_pressure() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(16, 0, 0);
    m.event_cursor = 1_100;
    m.freshness = fresh(1_000, 800, 1_200, 900, 1_400, false);
    let lookup = EntityRef::new(EntityKind::CacheObject, "lookup-master");
    let fetch = action("fetch_capsule", "Fetch cache capsule", RiskTier::ReadOnly);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Warning,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "cache_pressure".into(),
            severity: Severity::Warning,
            summary: "Cache hit ratio 41%; selector misses 312 in 24h".into(),
            entity: Some(lookup.clone()),
            recommended_action: Some(fetch.clone()),
        }),
        active_agents: 3,
        running_jobs: 6,
        queued_jobs: 18,
        open_capsules: 7,
        active_grants: 2,
        cache_hit_ratio: 0.41,
        selector_misses_24h: 312,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 38,
        ..MissionSnapshot::default()
    };
    m.attention.push(AttentionItem {
        id: "att-cache-low".into(),
        severity: Severity::Warning,
        title: "Cache hit ratio below 50%".into(),
        why_it_matters: "Cold cache slows pipelines; CI latency up.".into(),
        entity: lookup,
        evidence: vec!["capsule:cap-cache-stats".into()],
        recommended_actions: vec![fetch],
        created_at: ts(15, 30, 0),
        last_seen_at: ts(16, 0, 0),
    });
    m.system = cache_degraded_system("hit ratio 41%", 10, 2);
    m
}

/// Cache warm: hit ratio high, no taints, no selector misses recently.
pub fn cache_warm() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(16, 5, 0);
    m.event_cursor = 1_130;
    m.freshness = fresh(900, 700, 1_000, 800, 1_200, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        safe_to_release: true,
        active_agents: 4,
        running_jobs: 8,
        queued_jobs: 2,
        open_capsules: 4,
        active_grants: 2,
        cache_hit_ratio: 0.97,
        selector_misses_24h: 1,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 27,
        ..MissionSnapshot::default()
    };
    m.system = healthy_system(8, 4);
    m
}

/// Taint storm: many cache objects tainted, agents must skip cache.
pub fn taint_storm() -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(16, 10, 0);
    m.event_cursor = 1_215;
    m.freshness = fresh(1_100, 900, 1_300, 1_000, 1_500, false);
    let taint = EntityRef::new(EntityKind::CacheTaint, "taint-2026-05-26");
    let fetch = action("fetch_capsule", "Fetch taint capsule", RiskTier::ReadOnly);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Degraded,
        safe_to_code: true,
        safe_to_merge: false,
        safe_to_release: false,
        top_blocker: Some(BlockerSummary {
            kind: "taint_storm".into(),
            severity: Severity::Error,
            summary: "27 cache objects tainted in last hour; agents bypassing cache.".into(),
            entity: Some(taint.clone()),
            recommended_action: Some(fetch.clone()),
        }),
        active_agents: 2,
        blocked_agents: 1,
        running_jobs: 4,
        failed_jobs: 1,
        queued_jobs: 22,
        open_capsules: 11,
        active_grants: 3,
        cache_hit_ratio: 0.18,
        active_taints: 27,
        selector_misses_24h: 489,
        agents_can_code: true,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 52,
        taint_count: 27,
    };
    m.attention.push(AttentionItem {
        id: "att-taint-storm".into(),
        severity: Severity::Error,
        title: "Taint storm in progress".into(),
        why_it_matters: "Cache effectively disabled; CI latency 3x baseline.".into(),
        entity: taint.clone(),
        evidence: vec!["capsule:cap-taint-2026-05-26".into()],
        recommended_actions: vec![fetch.clone()],
        created_at: ts(15, 45, 0),
        last_seen_at: ts(16, 10, 0),
    });
    m.next_action = Some(NextActionRecommendation {
        action_ref: fetch,
        label: "Read taint storm capsule".into(),
        why: "Root cause needed before pruning taints.".into(),
        entity: Some(taint),
        confidence: 0.88,
        safety: ActionSafety::Safe,
        risk: RiskTier::ReadOnly,
    });
    m.system = cache_degraded_system("taint storm", 10, 2);
    m
}

fn cache_degraded_system(detail: &str, busy: u32, idle: u32) -> SystemHealth {
    let mut sys = healthy_system(busy, idle);
    sys.cache = degraded_component("cache", detail, 38);
    sys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dets(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn cache_pressure_is_deterministic() {
        assert_eq!(dets(&cache_pressure()), dets(&cache_pressure()));
    }

    #[test]
    fn cache_warm_is_deterministic() {
        assert_eq!(dets(&cache_warm()), dets(&cache_warm()));
    }

    #[test]
    fn taint_storm_is_deterministic() {
        assert_eq!(dets(&taint_storm()), dets(&taint_storm()));
    }

    #[test]
    fn cache_pressure_lowers_hit_ratio() {
        let m = cache_pressure();
        assert!(m.mission.cache_hit_ratio < 0.6);
        assert!(m.mission.selector_misses_24h > 100);
    }

    #[test]
    fn cache_warm_has_high_hit_ratio() {
        let m = cache_warm();
        assert!(m.mission.cache_hit_ratio > 0.9);
        assert_eq!(m.mission.active_taints, 0);
    }

    #[test]
    fn taint_storm_has_many_taints() {
        let m = taint_storm();
        assert!(m.mission.active_taints > 10);
        assert!(m.mission.cache_hit_ratio < 0.3);
    }
}
