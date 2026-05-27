//! Owner: Interactive TUI subsystem — Mission Control action adapter unit tests (Wave 6.A)
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter::tests::unit`
//! Invariants: Trait surface-level fake-adapter tests; no `App` wiring.

use super::super::{ActionAdapter, FakeActionAdapter, RecordedCall};
use crate::autonomy::signing::Signature;
use crate::autonomy::types::{GateDecision, LaunchLedgerEntry, LedgerKind, SchemaTag};
use chrono::Utc;

fn sample_entry(kind: LedgerKind, actor: &str) -> LaunchLedgerEntry {
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_test_{}", uuid::Uuid::new_v4()),
        kind,
        subject_id: "subj-1".into(),
        repo: Some("acme/widgets".into()),
        payload: serde_json::json!({"hello": "world"}),
        recorded_at: Utc::now(),
        actor: actor.into(),
        signature: Signature::stub(),
    }
}

#[tokio::test]
async fn fake_adapter_records_post_passport_check() {
    let fake = FakeActionAdapter::new();
    let id = fake
        .post_passport_check(
            "acme/widgets",
            "deadbeef",
            GateDecision::AllowMerge,
            "all green",
        )
        .await
        .expect("ok");
    assert!(id.contains("deadbeef"));
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        RecordedCall::PostPassportCheck {
            repo,
            head_sha,
            decision,
            summary,
        } => {
            assert_eq!(repo, "acme/widgets");
            assert_eq!(head_sha, "deadbeef");
            assert_eq!(*decision, GateDecision::AllowMerge);
            assert_eq!(summary, "all green");
        }
        other => panic!("expected PostPassportCheck, got {other:?}"),
    }
}

#[tokio::test]
async fn fake_adapter_returns_error_when_configured() {
    let fake = FakeActionAdapter::with_error_on("post_passport_check");
    let r = fake
        .post_passport_check("a/b", "sha", GateDecision::Reject, "bad")
        .await;
    assert!(r.is_err(), "expected injected error");
    // The call is still recorded so tests can assert the surface was hit.
    assert_eq!(fake.calls().len(), 1);
}

#[tokio::test]
async fn fake_adapter_records_append_ledger_with_snake_case_kind() {
    let fake = FakeActionAdapter::new();
    let entry = sample_entry(LedgerKind::HumanDecisionRecorded, "tui.cockpit.v1");
    fake.append_ledger(entry).await.expect("ok");
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    match &calls[0] {
        RecordedCall::AppendLedger { kind, actor, .. } => {
            assert_eq!(kind, "human_decision_recorded");
            assert_eq!(actor, "tui.cockpit.v1");
        }
        other => panic!("expected AppendLedger, got {other:?}"),
    }
}
