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
    let kinds = [
        EntityKind::Job,
        EntityKind::Pipeline,
        EntityKind::Agent,
        EntityKind::AgentTask,
        EntityKind::MergeRequest,
        EntityKind::TestPlan,
        EntityKind::TestCase,
        EntityKind::EvidenceCapsule,
        EntityKind::ReleaseAttempt,
        EntityKind::ReleaseGate,
        EntityKind::CacheTaint,
        EntityKind::CacheObject,
        EntityKind::Bug,
        EntityKind::BugAttempt,
        EntityKind::Project,
        EntityKind::SecretAccess,
        EntityKind::Grant,
        EntityKind::Pool,
        EntityKind::Runner,
        EntityKind::System,
    ];
    let mut labels = HashSet::new();
    for kind in &kinds {
        assert!(
            labels.insert(kind.label()),
            "duplicate label: {}",
            kind.label()
        );
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
