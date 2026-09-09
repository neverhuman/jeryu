//! save() / load_latest() round-trips, idempotency, and supersede-on-save.

use super::mint_verdict;
use crate::seam::VerdictStore;
use crate::types::GateDecision;
use crate::verdict_store::MemoryVerdictStore;
use chrono::{Duration, Utc};

#[tokio::test]
async fn save_then_load_latest_round_trips() {
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let v = mint_verdict(
        "owner/repo",
        Some("!42"),
        "abc12345",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(60),
    );
    store.save(&v).await.unwrap();
    let got = store
        .load_latest("owner/repo", Some("!42"))
        .await
        .unwrap()
        .expect("round-trip");
    assert_eq!(got.id, v.id);
    assert_eq!(got.pull_request, v.pull_request);
    assert_eq!(got.decision, GateDecision::AllowMerge);
}

#[tokio::test]
async fn save_is_idempotent_on_id() {
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let v = mint_verdict(
        "owner/repo",
        Some("!1"),
        "ffff0001",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(30),
    );
    store.save(&v).await.unwrap();
    store.save(&v).await.unwrap();
    store.save(&v).await.unwrap();
    let active = store.list_active(now).await.unwrap();
    assert_eq!(active.len(), 1, "same id must not insert twice");
    assert_eq!(active[0].id, v.id);
}

#[tokio::test]
async fn save_supersedes_prior_verdicts_for_same_repo_and_pr() {
    let store = MemoryVerdictStore::new();
    let t0 = Utc::now();
    let v1 = mint_verdict(
        "owner/repo",
        Some("!9"),
        "aaaa1111",
        GateDecision::AllowMerge,
        t0,
        t0 + Duration::minutes(60),
    );
    let v2 = mint_verdict(
        "owner/repo",
        Some("!9"),
        "bbbb2222",
        GateDecision::AllowMerge,
        t0 + Duration::seconds(5),
        t0 + Duration::minutes(60),
    );
    store.save(&v1).await.unwrap();
    store.save(&v2).await.unwrap();
    let got = store
        .load_latest("owner/repo", Some("!9"))
        .await
        .unwrap()
        .expect("latest");
    assert_eq!(got.id, v2.id, "newer save must win");
    let active = store.list_active(t0).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, v2.id);
}

#[tokio::test]
async fn load_latest_returns_none_for_unknown_pair() {
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let v = mint_verdict(
        "owner/repo",
        Some("!1"),
        "11112222",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(60),
    );
    store.save(&v).await.unwrap();
    assert!(
        store
            .load_latest("owner/other", Some("!1"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .load_latest("owner/repo", Some("!999"))
            .await
            .unwrap()
            .is_none()
    );
    // None vs Some("!1") are distinct.
    assert!(
        store
            .load_latest("owner/repo", None)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn body_json_is_source_of_truth_after_round_trip() {
    use crate::types::{ReviewDecision, ReviewerRole, VerdictReceiptRef};
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let mut v = mint_verdict(
        "owner/repo",
        Some("!42"),
        "beef0001",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(60),
    );
    v.hard_stops = vec!["security:high".into(), "tests:full_required".into()];
    v.approval_receipts = vec![
        VerdictReceiptRef {
            role: ReviewerRole::Security,
            agent_id: "reviewer-security.v1".into(),
            receipt_digest: "sha256:cafe".into(),
            decision: ReviewDecision::Pass,
            not_author: true,
        },
        VerdictReceiptRef {
            role: ReviewerRole::Judge,
            agent_id: "judge.v1".into(),
            receipt_digest: "sha256:beef".into(),
            decision: ReviewDecision::Pass,
            not_author: true,
        },
    ];
    store.save(&v).await.unwrap();
    let got = store
        .load_latest("owner/repo", Some("!42"))
        .await
        .unwrap()
        .expect("loads");
    assert_eq!(got, v, "body must round-trip every field losslessly");
}

#[tokio::test]
async fn save_with_unsigned_verdict_succeeds_for_replay_use_case() {
    use crate::signing::Signature;
    let store = MemoryVerdictStore::new();
    let now = Utc::now();
    let mut v = mint_verdict(
        "owner/repo",
        Some("!unsigned"),
        "0bad0001",
        GateDecision::AllowMerge,
        now,
        now + Duration::minutes(60),
    );
    v.signature = Signature::unsigned();
    store
        .save(&v)
        .await
        .expect("verdict_store accepts unsigned verdicts (replay use case)");
    let got = store
        .load_latest("owner/repo", Some("!unsigned"))
        .await
        .unwrap()
        .expect("round-trip");
    assert_eq!(got.signature.algo, "unsigned");
}
