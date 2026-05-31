use super::*;
use crate::ledger::MemoryLedger;
use crate::seam::LedgerFilter;

fn bell() -> (KillBell, Arc<MemoryLedger>) {
    let ledger = Arc::new(MemoryLedger::new());
    (KillBell::new(ledger.clone()), ledger)
}

fn key() -> EdSigningKey {
    EdSigningKey::generate("operator.kill-bell")
}

#[tokio::test]
async fn pause_then_is_paused_true() {
    let (bell, _l) = bell();
    let now = Utc::now();
    bell.pause("brown alert", "alice", 3600, &key(), now)
        .await
        .unwrap();
    assert!(bell.is_paused(now).await.unwrap());
    match bell.current(now).await.unwrap() {
        KillBellState::Paused {
            reason, paused_by, ..
        } => {
            assert_eq!(reason, "brown alert");
            assert_eq!(paused_by, "alice");
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn pause_with_ttl_expires_auto_arms() {
    let (bell, _l) = bell();
    let t0 = Utc::now();
    bell.pause("short pause", "bob", 1, &key(), t0)
        .await
        .unwrap();
    assert!(bell.is_paused(t0).await.unwrap(), "paused at t0");
    let t_later = t0 + Duration::seconds(5);
    assert_eq!(
        bell.current(t_later).await.unwrap(),
        KillBellState::Armed,
        "expired TTL must auto-arm to prevent permanent brick"
    );
    assert!(!bell.is_paused(t_later).await.unwrap());
}

#[tokio::test]
async fn resume_clears_paused() {
    let (bell, _l) = bell();
    let now = Utc::now();
    bell.pause("incident", "alice", 3600, &key(), now)
        .await
        .unwrap();
    assert!(bell.is_paused(now).await.unwrap());
    bell.resume("alice", &key(), now + Duration::seconds(10))
        .await
        .unwrap();
    assert_eq!(
        bell.current(now + Duration::seconds(20)).await.unwrap(),
        KillBellState::Armed,
        "explicit resume must clear paused even before TTL"
    );
}

#[tokio::test]
async fn downgrade_if_paused_downgrades_allow_merge() {
    let (bell, _l) = bell();
    let now = Utc::now();
    bell.pause("freeze", "alice", 3600, &key(), now)
        .await
        .unwrap();
    let (decision, why) = bell
        .downgrade_if_paused(GateDecision::AllowMerge, now)
        .await
        .unwrap();
    assert_eq!(decision, GateDecision::RequireHuman);
    let why = why.expect("paused must surface a reason");
    assert!(why.contains("freeze"), "reason should round-trip: {why}");
    assert!(why.contains("alice"));
}

#[tokio::test]
async fn downgrade_if_paused_passes_through_when_armed() {
    let (bell, _l) = bell();
    let now = Utc::now();
    let (decision, why) = bell
        .downgrade_if_paused(GateDecision::AllowMerge, now)
        .await
        .unwrap();
    assert_eq!(decision, GateDecision::AllowMerge);
    assert!(why.is_none(), "armed must not surface a reason");
    let (decision, why) = bell
        .downgrade_if_paused(GateDecision::Reject, now)
        .await
        .unwrap();
    assert_eq!(decision, GateDecision::Reject);
    assert!(why.is_none());
}

#[tokio::test]
async fn pause_appends_signed_ledger_entry_with_kill_bell_engaged_kind() {
    let (bell, ledger) = bell();
    let now = Utc::now();
    bell.pause("network split", "alice", 60, &key(), now)
        .await
        .unwrap();
    let entries = ledger
        .list(&LedgerFilter {
            kind: Some(LedgerKind::KillBellEngaged),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].subject_id, "kill_bell");
    assert_eq!(entries[0].actor, "alice");
    assert_eq!(entries[0].payload["reason"], "network split");
    assert_eq!(entries[0].payload["ttl_seconds"], 60);
    // The ledger entry is ed25519-signed (the ledger refuses anything else).
    assert_eq!(entries[0].signature.algo, "ed25519");
    assert_ne!(entries[0].signature.value, "0".repeat(64));
}

#[tokio::test]
async fn resume_appends_ledger_entry_with_kill_bell_resumed_kind() {
    let (bell, ledger) = bell();
    let now = Utc::now();
    bell.pause("ttest", "alice", 60, &key(), now).await.unwrap();
    bell.resume("bob", &key(), now + Duration::seconds(5))
        .await
        .unwrap();
    let entries = ledger
        .list(&LedgerFilter {
            kind: Some(LedgerKind::KillBellResumed),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].actor, "bob");
    assert_eq!(entries[0].payload["resumed_by"], "bob");
}

#[tokio::test]
async fn pause_during_pause_appends_and_latest_window_wins() {
    let (bell, ledger) = bell();
    let now = Utc::now();
    bell.pause("first", "alice", 60, &key(), now).await.unwrap();
    bell.pause("second", "bob", 120, &key(), now + Duration::seconds(5))
        .await
        .unwrap();
    let entries = ledger
        .list(&LedgerFilter {
            kind: Some(LedgerKind::KillBellEngaged),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 2, "both pauses must each leave a receipt");
    match bell.current(now + Duration::seconds(10)).await.unwrap() {
        KillBellState::Paused {
            reason, paused_by, ..
        } => {
            assert_eq!(reason, "second", "latest pause's reason must surface");
            assert_eq!(paused_by, "bob");
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[tokio::test]
async fn status_query_consistency_across_apis() {
    let (bell, _l) = bell();
    let now = Utc::now();
    bell.pause("freeze", "alice", 3600, &key(), now)
        .await
        .unwrap();
    let probe = now + Duration::seconds(30);
    let cur_paused = matches!(
        bell.current(probe).await.unwrap(),
        KillBellState::Paused { .. }
    );
    let is_paused = bell.is_paused(probe).await.unwrap();
    let (decision, why) = bell
        .downgrade_if_paused(GateDecision::AllowMerge, probe)
        .await
        .unwrap();
    assert_eq!(cur_paused, is_paused, "current()/is_paused() must agree");
    assert!(cur_paused);
    assert_eq!(decision, GateDecision::RequireHuman);
    assert!(why.is_some());
}
