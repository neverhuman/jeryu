//! End-to-end serde round-trip + provider-neutrality tests over the full
//! sample read model.

use jeryu_readmodel::{SCHEMA_VERSION, TuiReadModel, sample_read_model};

#[test]
fn sample_read_model_round_trips_json() {
    let model = sample_read_model();
    let encoded = serde_json::to_string(&model).expect("serialize");
    let decoded: TuiReadModel = serde_json::from_str(&encoded).expect("deserialize");

    // Spot-check that the nested contract survived the round trip.
    assert_eq!(decoded.schema_version, SCHEMA_VERSION);
    assert_eq!(decoded.event_cursor, model.event_cursor);
    assert_eq!(decoded.mission.active_agents, model.mission.active_agents);
    assert_eq!(decoded.queue.waiting_jobs.len(), 1);
    assert_eq!(decoded.repos.families.len(), 1);
    assert_eq!(decoded.runners.items.len(), 1);
    assert_eq!(decoded.source_doctor.items.len(), 1);
    assert_eq!(decoded.attention.len(), 1);
    assert!(decoded.next_action.is_some());
    assert_eq!(decoded.system.scm.name, "scm");
}

#[test]
fn default_read_model_round_trips_json() {
    let model = TuiReadModel::default();
    let encoded = serde_json::to_string(&model).expect("serialize");
    let decoded: TuiReadModel = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded.schema_version, model.schema_version);
    assert_eq!(decoded.repos, model.repos);
    assert_eq!(decoded.queue, model.queue);
}

#[test]
fn wire_shape_has_no_provider_named_fields() {
    let json = serde_json::to_string(&sample_read_model()).unwrap();
    let lowered = json.to_lowercase();
    // D1: forbidden provider literals must not appear on the wire. The needles
    // are assembled at runtime so no forbidden literal exists in the source.
    let forbidden = [
        format!("git{}", "lab"),
        format!("jit{}", "forge"),
        format!("ni{}", "tro"),
    ];
    for needle in &forbidden {
        assert!(!lowered.contains(needle), "found forbidden literal on wire");
    }
    // Provider-neutral source-control field is present.
    assert!(
        json.contains("\"scm\""),
        "expected scm component on the wire"
    );
}

#[test]
fn unknown_optional_dashboards_default_in() {
    // A minimal payload missing the #[serde(default)] dashboards must decode.
    let minimal = r#"{
        "schema_version": "tui.v1.0",
        "generated_at": "2026-05-30T12:00:00Z",
        "event_cursor": 0,
        "freshness": {"overall_outdated": false},
        "mission": {
            "overall": "healthy",
            "safe_to_code": true,
            "safe_to_merge": false,
            "safe_to_release": false,
            "top_blocker": null,
            "active_agents": 0,
            "blocked_agents": 0,
            "running_jobs": 0,
            "failed_jobs": 0,
            "queued_jobs": 0,
            "open_capsules": 0,
            "active_grants": 0,
            "cache_hit_ratio": 0.0,
            "active_taints": 0,
            "selector_misses_24h": 0,
            "agents_can_code": true,
            "active_runners": 0,
            "total_runners": 0,
            "evidence_count": 0,
            "taint_count": 0
        },
        "attention": [],
        "next_action": null,
        "system": {
            "scm": {"name": "scm", "status": "healthy", "latency_ms": 1, "detail": null},
            "database": {"name": "database", "status": "healthy", "latency_ms": 1, "detail": null},
            "sandbox": {"name": "sandbox", "status": "healthy", "latency_ms": 1, "detail": null},
            "cache": {"name": "cache", "status": "healthy", "latency_ms": 1, "detail": null},
            "vault": {"name": "vault", "status": "healthy", "latency_ms": 1, "detail": null},
            "runners": {"online": 0, "busy": 0, "idle": 0, "degraded": 0}
        }
    }"#;
    let decoded: TuiReadModel = serde_json::from_str(minimal).expect("decode minimal");
    assert!(decoded.queue.pools.is_empty());
    assert!(decoded.runners.items.is_empty());
    assert!(decoded.source_doctor.items.is_empty());
    assert_eq!(decoded.repos.registry_path, "");
}
