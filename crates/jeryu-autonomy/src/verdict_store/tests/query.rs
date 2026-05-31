//! list_active() filtering/ordering, explicit supersede(), and concurrency.

use super::mint_verdict;
use crate::seam::VerdictStore;
use crate::types::GateDecision;
use crate::verdict_store::MemoryVerdictStore;
use chrono::{Duration, Utc};

#[tokio::test]
async fn list_active_excludes_expired_verdicts() {
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let expired = mint_verdict(
        "owner/repo",
        Some("!1"),
        "11110000",
        GateDecision::AllowMerge,
        now - Duration::minutes(120),
        now - Duration::minutes(60),
    );
    let live = mint_verdict(
        "owner/repo",
        Some("!2"),
        "22220000",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(60),
    );
    store.save(&expired).await.unwrap();
    store.save(&live).await.unwrap();
    let active = store.list_active(now).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, live.id);
}

#[tokio::test]
async fn list_active_excludes_rejected_verdicts() {
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let allow = mint_verdict(
        "owner/repo",
        Some("!a"),
        "aaaa0000",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(60),
    );
    let reject = mint_verdict(
        "owner/repo",
        Some("!r"),
        "ffff0000",
        GateDecision::Reject,
        now,
        now + Duration::minutes(60),
    );
    let human = mint_verdict(
        "owner/repo",
        Some("!h"),
        "cccc0000",
        GateDecision::RequireHuman,
        now,
        now + Duration::minutes(60),
    );
    store.save(&allow).await.unwrap();
    store.save(&reject).await.unwrap();
    store.save(&human).await.unwrap();
    let active = store.list_active(now).await.unwrap();
    let ids: Vec<&str> = active.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(active.len(), 2, "reject must be excluded; got ids={ids:?}");
    assert!(ids.contains(&allow.id.as_str()));
    assert!(ids.contains(&human.id.as_str()));
    assert!(!ids.contains(&reject.id.as_str()));
}

#[tokio::test]
async fn list_active_orders_by_created_at_ascending() {
    let store = MemoryVerdictStore::new();
    let t0 = Utc::now();
    let v_b = mint_verdict(
        "owner/repo",
        Some("!b"),
        "bbbb0001",
        GateDecision::AllowMerge,
        t0 + Duration::seconds(20),
        t0 + Duration::minutes(60),
    );
    let v_a = mint_verdict(
        "owner/repo",
        Some("!a"),
        "aaaa0001",
        GateDecision::AllowMerge,
        t0 + Duration::seconds(10),
        t0 + Duration::minutes(60),
    );
    let v_c = mint_verdict(
        "owner/repo",
        Some("!c"),
        "cccc0001",
        GateDecision::AllowMerge,
        t0 + Duration::seconds(30),
        t0 + Duration::minutes(60),
    );
    store.save(&v_c).await.unwrap();
    store.save(&v_a).await.unwrap();
    store.save(&v_b).await.unwrap();
    let active = store.list_active(t0).await.unwrap();
    assert_eq!(active.len(), 3);
    assert_eq!(active[0].id, v_a.id, "earliest first");
    assert_eq!(active[1].id, v_b.id);
    assert_eq!(active[2].id, v_c.id, "latest last");
}

#[tokio::test]
async fn supersede_marks_row_and_is_idempotent() {
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let v = mint_verdict(
        "owner/repo",
        Some("!1"),
        "fade0001",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(60),
    );
    store.save(&v).await.unwrap();
    assert_eq!(store.list_active(now).await.unwrap().len(), 1);
    store
        .supersede(&v.id, now + Duration::seconds(5))
        .await
        .unwrap();
    assert_eq!(store.list_active(now).await.unwrap().len(), 0);
    store
        .supersede(&v.id, now + Duration::seconds(10))
        .await
        .expect("idempotent");
    store
        .supersede("vgv_nope", now)
        .await
        .expect("unknown id is a no-op");
}

#[tokio::test]
async fn concurrent_save_no_corruption_with_four_tasks() {
    let store = MemoryVerdictStore::new();
    let t0 = Utc::now();
    let mut handles = Vec::new();
    for task in 0..4 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..5 {
                let v = mint_verdict(
                    "owner/repo",
                    Some(&format!("!t{task}-{i}")),
                    &format!("t{task}i{i:03}"),
                    GateDecision::AllowMerge,
                    t0 + Duration::milliseconds((task * 100 + i) as i64),
                    t0 + Duration::minutes(60),
                );
                store.save(&v).await.expect("concurrent save");
            }
        }));
    }
    for h in handles {
        h.await.expect("task joined");
    }
    let active = store.list_active(t0).await.unwrap();
    assert_eq!(
        active.len(),
        20,
        "4 tasks * 5 distinct verdicts must produce 20 active rows"
    );
    let got = store
        .load_latest("owner/repo", Some("!t2-3"))
        .await
        .unwrap()
        .expect("pair exists");
    assert_eq!(got.pull_request.as_deref(), Some("!t2-3"));
}
