use super::*;

#[test]
fn entity_ref_display() {
    let r = EntityRef::new(EntityKind::Job, "14445");
    assert_eq!(r.display(), "job:14445");
    assert_eq!(format!("{r}"), "job:14445");
}

#[test]
fn entity_kinds_have_unique_labels() {
    use std::collections::HashSet;
    let mut labels = HashSet::new();
    for kind in EntityKind::ALL {
        assert!(
            labels.insert(kind.label()),
            "duplicate label: {}",
            kind.label()
        );
    }
}

#[test]
fn entity_kinds_have_routes_and_badges() {
    for kind in EntityKind::ALL {
        assert!(
            !kind.route_segment().is_empty(),
            "missing route for {kind:?}"
        );
        assert!(!kind.badge().is_empty(), "missing badge for {kind:?}");
    }
}

#[test]
fn legacy_entity_kind_json_still_deserializes() {
    let fixtures = [
        ("\"job\"", EntityKind::Job),
        ("\"pipeline\"", EntityKind::Pipeline),
        ("\"agent\"", EntityKind::Agent),
        ("\"agent_task\"", EntityKind::AgentTask),
        ("\"merge_request\"", EntityKind::MergeRequest),
        ("\"test_plan\"", EntityKind::TestPlan),
        ("\"test_case\"", EntityKind::TestCase),
        ("\"evidence_capsule\"", EntityKind::EvidenceCapsule),
        ("\"release_attempt\"", EntityKind::ReleaseAttempt),
        ("\"release_gate\"", EntityKind::ReleaseGate),
        ("\"cache_taint\"", EntityKind::CacheTaint),
        ("\"cache_object\"", EntityKind::CacheObject),
        ("\"bug\"", EntityKind::Bug),
        ("\"bug_attempt\"", EntityKind::BugAttempt),
        ("\"project\"", EntityKind::Project),
        ("\"secret_access\"", EntityKind::SecretAccess),
        ("\"grant\"", EntityKind::Grant),
        ("\"pool\"", EntityKind::Pool),
        ("\"runner\"", EntityKind::Runner),
        ("\"system\"", EntityKind::System),
    ];

    for (json, expected) in fixtures {
        let actual: EntityKind = serde_json::from_str(json).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn severity_ordering() {
    assert!(Severity::Critical < Severity::Error);
    assert!(Severity::Error < Severity::Warning);
    assert!(Severity::Warning < Severity::Info);
}

#[test]
fn entity_detail_default_is_unknown() {
    let detail = EntityDetail::default();
    assert_eq!(detail.state, "unknown");
    assert!(detail.timeline.is_empty());
    assert!(detail.blockers.is_empty());
}
