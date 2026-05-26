//! Owner: Interactive TUI subsystem — adapter unit tests
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter::tests::unit`
//! Invariants:
//!   - Tests in this file exercise the `FakeActionAdapter` and
//!     `ActionAdapter::kind()` surface directly — no `App` integration.
//!   - Each test asserts both the recorded call shape AND the returned
//!     value, so refactors that drop the recording silently fail loudly.

use super::sample_entry;
use crate::autonomy::types::{GateDecision, LedgerKind};
use crate::tui::workflow::action_adapter::{ActionAdapter, FakeActionAdapter, RecordedCall};

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

#[test]
fn fake_adapter_kind_returns_fake() {
    let fake = FakeActionAdapter::new();
    assert_eq!(fake.kind(), "fake");
}

#[test]
fn app_action_adapter_is_send_sync_for_tokio() {
    // Compile-time guarantee: the field type must be Send + Sync so
    // tokio tasks can `tokio::spawn` work that holds an `Arc<dyn
    // ActionAdapter>` (the auto-rejudge / background-sync paths in the
    // App). If anyone makes the trait `?Send`, this fails to compile.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<std::sync::Arc<dyn ActionAdapter>>();
}
