use super::*;
use crate::api::entity::{EntityKind, EntityRef};

fn make_event(kind: TuiEventKind, summary: &str) -> TuiEvent {
    TuiEvent {
        seq: 0, // will be assigned by EventStore::push
        timestamp: Utc::now(),
        kind,
        severity: Severity::Info,
        entity: EntityRef::new(EntityKind::System, "test"),
        parent: None,
        summary: summary.into(),
        correlation_id: None,
        evidence_refs: Vec::new(),
        next_actions: Vec::new(),
        stale_after_ms: 5000,
    }
}

#[test]
fn event_store_assigns_monotonic_seqs() {
    let mut store = EventStore::new(100);
    let s1 = store.push(make_event(TuiEventKind::SystemHealthUpdated, "a"));
    let s2 = store.push(make_event(TuiEventKind::JobFailed, "b"));
    let s3 = store.push(make_event(TuiEventKind::AgentSessionCreated, "c"));
    assert_eq!(s1, 1);
    assert_eq!(s2, 2);
    assert_eq!(s3, 3);
    assert_eq!(store.cursor(), 3);
    assert_eq!(store.len(), 3);
}

#[test]
fn event_store_respects_capacity() {
    let mut store = EventStore::new(2);
    store.push(make_event(TuiEventKind::JobStarted, "first"));
    store.push(make_event(TuiEventKind::JobFailed, "second"));
    store.push(make_event(TuiEventKind::JobRetried, "third"));
    assert_eq!(store.len(), 2);
    let events: Vec<_> = store.all().collect();
    assert_eq!(events[0].summary, "second");
    assert_eq!(events[1].summary, "third");
}

#[test]
fn event_store_since_filters_correctly() {
    let mut store = EventStore::new(100);
    store.push(make_event(TuiEventKind::JobStarted, "a"));
    store.push(make_event(TuiEventKind::JobFailed, "b"));
    store.push(make_event(TuiEventKind::JobRetried, "c"));
    let since_1: Vec<_> = store.since(1).collect();
    assert_eq!(since_1.len(), 2);
    assert_eq!(since_1[0].summary, "b");
    assert_eq!(since_1[1].summary, "c");
}

#[test]
fn event_store_recent_returns_newest_first() {
    let mut store = EventStore::new(100);
    store.push(make_event(TuiEventKind::JobStarted, "prior"));
    store.push(make_event(TuiEventKind::JobFailed, "mid"));
    store.push(make_event(TuiEventKind::JobRetried, "new"));
    let recent = store.recent(2);
    assert_eq!(recent[0].summary, "new");
    assert_eq!(recent[1].summary, "mid");
}

#[test]
fn event_kind_labels_are_dot_separated() {
    assert_eq!(TuiEventKind::JobFailed.label(), "job.failed");
    assert_eq!(
        TuiEventKind::RunnerNodeUnreachable.label(),
        "runner.node.unreachable"
    );
    assert_eq!(
        TuiEventKind::RunnerNodeBackOnline.label(),
        "runner.node.back_online"
    );
    assert_eq!(
        TuiEventKind::FleetUnderfilled.label(),
        "runner.fleet.underfilled"
    );
    assert_eq!(
        TuiEventKind::AgentRaceWinnerSelected.label(),
        "agent.race.winner"
    );
    assert_eq!(
        TuiEventKind::TestVtiAccelerated.label(),
        "test.vti.accelerated"
    );
    assert_eq!(
        TuiEventKind::RunnerOrphanedDetected.label(),
        "runner.orphaned.detected"
    );
    assert_eq!(
        TuiEventKind::HungRunnerDetected.label(),
        "runner.hung.detected"
    );
}

#[test]
fn event_kind_runner_lifecycle_variants_round_trip_through_json() {
    for kind in [
        TuiEventKind::RunnerNodeUnreachable,
        TuiEventKind::RunnerNodeBackOnline,
        TuiEventKind::FleetUnderfilled,
        TuiEventKind::RunnerDiskCritical,
        TuiEventKind::RunnerOrphanedDetected,
        TuiEventKind::HungRunnerDetected,
    ] {
        let json = serde_json::to_string(&kind).unwrap();
        let back: TuiEventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind);
    }
}
