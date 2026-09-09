use super::*;
use crate::signing::EdSigningKey;
use crate::types::{RiskTier, VerdictReceiptRef};
use chrono::{Duration, Utc};

fn signed_entry(id: &str, kind: LedgerKind) -> LaunchLedgerEntry {
    let key = EdSigningKey::generate("test-agent");
    let mut e = LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: id.into(),
        kind,
        subject_id: "subj-1".into(),
        repo: Some("owner/repo".into()),
        payload: serde_json::json!({"hello": "world"}),
        recorded_at: Utc::now(),
        actor: "judge.v1".into(),
        signature: Signature::default_unsigned(),
    };
    sign_entry(&mut e, &key);
    e
}

#[tokio::test]
async fn append_and_list_roundtrip() {
    let ledger = MemoryLedger::new();
    let e = signed_entry("evt-1", LedgerKind::VerdictIssued);
    ledger.append(&e).await.unwrap();
    let got = ledger.list(&LedgerFilter::default()).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "evt-1");
    assert_eq!(got[0].kind, LedgerKind::VerdictIssued);
    assert_eq!(got[0].subject_id, "subj-1");
    assert_eq!(got[0].payload, serde_json::json!({"hello": "world"}));
    assert_eq!(got[0].signature.algo, "ed25519");
}

#[tokio::test]
async fn append_is_idempotent_on_id() {
    let ledger = MemoryLedger::new();
    let e = signed_entry("evt-dup", LedgerKind::VerdictIssued);
    ledger.append(&e).await.unwrap();
    ledger.append(&e).await.unwrap();
    let got = ledger.list(&LedgerFilter::default()).await.unwrap();
    assert_eq!(got.len(), 1, "duplicate id must not insert twice");
}

#[tokio::test]
async fn append_refuses_unsigned_signature() {
    let ledger = MemoryLedger::new();
    let mut e = signed_entry("evt-unsigned", LedgerKind::VerdictIssued);
    e.signature = Signature::unsigned();
    let err = ledger.append(&e).await.unwrap_err();
    assert!(err.to_string().contains("unsigned"), "actual: {err}");
}

#[tokio::test]
async fn append_refuses_hmac_signature() {
    let ledger = MemoryLedger::new();
    let mut e = signed_entry("evt-hmac", LedgerKind::VerdictIssued);
    e.signature = Signature {
        algo: "hmac-sha256-insecure".into(),
        key_id: "k".into(),
        value: "0".repeat(64),
    };
    let err = ledger.append(&e).await.unwrap_err();
    assert!(
        err.to_string().contains("hmac-sha256-insecure"),
        "actual: {err}"
    );
}

/// Append-only invariant: once a row is written, re-appending the same id
/// (even with a different body) does NOT mutate the stored row. There is no
/// update/delete API — mirrors the SQL trigger that aborts UPDATE/DELETE.
#[tokio::test]
async fn append_only_no_mutation_after_write() {
    let ledger = MemoryLedger::new();
    let original = signed_entry("evt-x", LedgerKind::VerdictIssued);
    ledger.append(&original).await.unwrap();
    // Attempt to "overwrite" with a tampered body under the same id.
    let mut tampered = original.clone();
    tampered.actor = "hacker".into();
    ledger.append(&tampered).await.unwrap(); // no-op
    let got = ledger.list(&LedgerFilter::default()).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(
        got[0].actor, "judge.v1",
        "row must be immutable after append"
    );
}

#[tokio::test]
async fn verdict_round_trip_signs_and_appends() {
    let ledger = MemoryLedger::new();
    let now = Utc::now();
    let verdict = VibeGateVerdict {
        schema: SchemaTag::new(),
        id: "vgv_abc".into(),
        evidence_pack_id: "ep_1".into(),
        pull_request: Some("!42".into()),
        repo: "owner/repo".into(),
        target_branch: "main".into(),
        head_sha: "a".repeat(40),
        policy_sha: "c".repeat(40),
        evidence_pack_digest: "sha256:deadbeef".into(),
        risk: RiskTier::R2,
        hard_stops: vec![],
        required_reviews: vec![],
        approval_receipts: Vec::<VerdictReceiptRef>::new(),
        decision: GateDecision::AllowMerge,
        valid_for_head_sha_only: true,
        rebind_on_train: true,
        expires_at: now + Duration::minutes(60),
        created_at: now,
        signature: Signature::unsigned(),
    };
    let key = EdSigningKey::generate("judge.v1");
    let mut entry = verdict_issued_entry(&verdict, "judge.v1");
    // Before signing, append must refuse (unsigned algo).
    assert!(ledger.append(&entry).await.is_err());
    sign_entry(&mut entry, &key);
    ledger.append(&entry).await.unwrap();
    let got = ledger.list(&LedgerFilter::default()).await.unwrap();
    assert_eq!(got.len(), 1);
    let body = canonical_body_for_signing(&got[0]);
    assert!(
        key.verifier().verify(body.as_bytes(), &got[0].signature),
        "ed25519 signature must verify after round-trip"
    );
}

#[tokio::test]
async fn list_filters_by_kind_and_subject() {
    let ledger = MemoryLedger::new();
    ledger
        .append(&signed_entry("a", LedgerKind::VerdictIssued))
        .await
        .unwrap();
    let mut other = signed_entry("b", LedgerKind::RollbackInitiated);
    other.subject_id = "subj-2".into();
    ledger.append(&other).await.unwrap();

    let verdicts = ledger
        .list(&LedgerFilter {
            kind: Some(LedgerKind::VerdictIssued),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].id, "a");

    let subj_2 = ledger
        .list(&LedgerFilter {
            subject_id: Some("subj-2".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(subj_2.len(), 1);
    assert_eq!(subj_2[0].id, "b");
}

#[tokio::test]
async fn concurrent_append_no_corruption_with_four_tasks() {
    let ledger = MemoryLedger::new();
    let mut handles = Vec::new();
    for task in 0..4 {
        let ledger = ledger.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..5 {
                let id = format!("evt-t{task}-{i}");
                let e = signed_entry(&id, LedgerKind::VerdictIssued);
                ledger.append(&e).await.expect("concurrent append");
            }
        }));
    }
    for h in handles {
        h.await.expect("task joined");
    }
    let got = ledger.list(&LedgerFilter::default()).await.unwrap();
    assert_eq!(
        got.len(),
        20,
        "4 tasks * 5 entries should produce exactly 20 rows"
    );
    let unique: std::collections::HashSet<_> = got.iter().map(|e| e.id.clone()).collect();
    assert_eq!(unique.len(), 20, "no duplicate ids must survive");
}

#[tokio::test]
async fn list_empty_filter_match_returns_empty_vec() {
    let ledger = MemoryLedger::new();
    ledger
        .append(&signed_entry("only-one", LedgerKind::VerdictIssued))
        .await
        .unwrap();
    let got = ledger
        .list(&LedgerFilter {
            subject_id: Some("does-not-exist".into()),
            ..Default::default()
        })
        .await
        .expect("empty result must be Ok");
    assert!(got.is_empty());
}

#[tokio::test]
async fn list_limit_boundary_zero_and_one() {
    let ledger = MemoryLedger::new();
    for i in 0..5 {
        let e = signed_entry(&format!("evt-{i}"), LedgerKind::VerdictIssued);
        ledger.append(&e).await.unwrap();
    }
    let none = ledger
        .list(&LedgerFilter {
            limit: Some(0),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(none.len(), 0);
    let one = ledger
        .list(&LedgerFilter {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(one.len(), 1);
}

#[test]
fn kind_to_str_handles_webhook_received() {
    assert_eq!(kind_to_str(LedgerKind::WebhookReceived), "webhook_received");
    let back = kind_from_str("webhook_received").expect("decodes");
    assert_eq!(back, LedgerKind::WebhookReceived);
    assert_ne!(
        kind_to_str(LedgerKind::WebhookReceived),
        kind_to_str(LedgerKind::HumanDecisionRecorded)
    );
}

#[tokio::test]
async fn append_then_list_with_webhook_received_kind() {
    let ledger = MemoryLedger::new();
    let entry = signed_entry("wh-1", LedgerKind::WebhookReceived);
    ledger.append(&entry).await.expect("append webhook entry");
    let got = ledger
        .list(&LedgerFilter {
            kind: Some(LedgerKind::WebhookReceived),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "wh-1");
    let human = ledger
        .list(&LedgerFilter {
            kind: Some(LedgerKind::HumanDecisionRecorded),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        human.is_empty(),
        "webhook entries must NOT leak into the human-decision stream"
    );
}

#[tokio::test]
async fn list_returns_err_on_malformed_json_payload() {
    let ledger = MemoryLedger::new();
    let e = signed_entry("evt-bad-json", LedgerKind::VerdictIssued);
    ledger.append(&e).await.unwrap();
    ledger.corrupt_payload_of("evt-bad-json");
    let err = ledger
        .list(&LedgerFilter::default())
        .await
        .expect_err("malformed payload must surface as an Err");
    let msg = format!("{err}");
    assert!(
        msg.contains("decode") || msg.contains("payload"),
        "error should reference payload decoding; got: {msg}"
    );
}
