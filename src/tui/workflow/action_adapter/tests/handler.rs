//! Owner: Interactive TUI subsystem — handle_delivery_action integration tests
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter::tests::handler`
//! Invariants:
//!   - These tests bind a real `App` (in-memory store) to a
//!     `FakeActionAdapter` so we can assert the wave-6.A behaviour spec
//!     end-to-end without any network or signed-ledger SQL setup.
//!   - The `app_with_demo_delivery` factory is the only path to the
//!     `App`; do not synthesize alternative fixtures inline.

use crate::autonomy::types::GateDecision;
use crate::tui::workflow::action_adapter::{FakeActionAdapter, RecordedCall};
use crate::tui::workflow::actions::{ActionOutcome, DeliveryAction};
use crate::tui::workflow::delivery::build_demo_delivery;

async fn app_with_demo_delivery() -> crate::tui::app::App {
    let mut app = crate::tui::app::test_app()
        .await
        .expect("build in-memory test app");
    app.delivery_snapshot = build_demo_delivery();
    app
}

fn ledger_kinds(fake: &FakeActionAdapter) -> Vec<String> {
    fake.calls()
        .into_iter()
        .filter_map(|c| match c {
            RecordedCall::AppendLedger { kind, .. } => Some(kind),
            _ => None,
        })
        .collect()
}

fn call_names(fake: &FakeActionAdapter) -> Vec<&'static str> {
    fake.calls()
        .into_iter()
        .map(|c| match c {
            RecordedCall::PostPassportCheck { .. } => "post_passport_check",
            RecordedCall::PostMrComment { .. } => "post_mr_comment",
            RecordedCall::PauseKillBell { .. } => "pause_kill_bell",
            RecordedCall::AppendLedger { .. } => "append_ledger",
        })
        .collect()
}

#[tokio::test]
async fn handle_approve_once_calls_post_passport_check_with_allow_merge() {
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(DeliveryAction::ApproveOnce { pr_idx: 0 }, &fake)
        .await;
    let calls = fake.calls();
    // First call must be the passport check with AllowMerge.
    match &calls[0] {
        RecordedCall::PostPassportCheck { decision, .. } => {
            assert_eq!(*decision, GateDecision::AllowMerge);
        }
        other => panic!("expected PostPassportCheck first, got {other:?}"),
    }
    // The action pane should show Submitted.
    assert!(matches!(
        app.action_pane.last_result.as_ref().map(|r| &r.outcome),
        Some(ActionOutcome::Submitted)
    ));
}

#[tokio::test]
async fn handle_block_verdict_calls_passport_check_then_comment() {
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(
        DeliveryAction::BlockVerdict {
            pr_idx: 0,
            reason: "regression in checkout".into(),
        },
        &fake,
    )
    .await;
    let names = call_names(&fake);
    // Exact order: passport_check (Reject) → comment → ledger
    assert_eq!(
        &names[..3],
        &["post_passport_check", "post_mr_comment", "append_ledger"],
        "BLOCK must passport-check first, then comment, then ledger; got {names:?}",
    );
    // Reject decision and reason are surfaced in the call args.
    match &fake.calls()[0] {
        RecordedCall::PostPassportCheck {
            decision, summary, ..
        } => {
            assert_eq!(*decision, GateDecision::Reject);
            assert!(summary.contains("regression in checkout"));
        }
        other => panic!("expected PostPassportCheck, got {other:?}"),
    }
    match &fake.calls()[1] {
        RecordedCall::PostMrComment { body, .. } => {
            assert!(body.contains("regression in checkout"));
            assert!(body.starts_with("Agent: BLOCK"));
        }
        other => panic!("expected PostMrComment, got {other:?}"),
    }
}

#[tokio::test]
async fn handle_request_repair_calls_post_mr_comment_only() {
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(DeliveryAction::RequestRepair { pr_idx: 0 }, &fake)
        .await;
    let names = call_names(&fake);
    // Repair must NOT touch the passport check.
    assert!(
        !names.contains(&"post_passport_check"),
        "RequestRepair must not post a passport check; got {names:?}"
    );
    assert_eq!(
        &names[..2],
        &["post_mr_comment", "append_ledger"],
        "RequestRepair = comment + ledger; got {names:?}"
    );
    match &fake.calls()[0] {
        RecordedCall::PostMrComment { body, .. } => {
            assert!(
                body.contains("repair this MR"),
                "repair-comment body should reference repair: {body}"
            );
        }
        other => panic!("expected PostMrComment, got {other:?}"),
    }
}

#[tokio::test]
async fn handle_freeze_autonomy_appends_ledger_event_no_adapter_call() {
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(DeliveryAction::FreezeAutonomy { hours: 12 }, &fake)
        .await;
    let calls = fake.calls();
    // Freeze must ONLY append a ledger event (no passport / comment /
    // kill-bell calls). Ops finalizes via CLI.
    assert_eq!(calls.len(), 1, "freeze emits exactly one ledger row");
    match &calls[0] {
        RecordedCall::AppendLedger { kind, payload, .. } => {
            assert_eq!(kind, "human_escalation_requested");
            assert_eq!(payload["action"], "freeze_intent");
            assert_eq!(payload["hours"], 12);
        }
        other => panic!("expected AppendLedger only, got {other:?}"),
    }
}

#[tokio::test]
async fn handle_kill_bell_calls_pause_then_returns_submitted() {
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(
        DeliveryAction::KillBell {
            reason: "incident-42".into(),
        },
        &fake,
    )
    .await;
    let calls = fake.calls();
    assert_eq!(
        calls.len(),
        1,
        "KillBell must invoke exactly one adapter call"
    );
    match &calls[0] {
        RecordedCall::PauseKillBell {
            reason,
            paused_by,
            ttl_seconds,
        } => {
            assert_eq!(reason, "incident-42");
            assert_eq!(paused_by, "tui.cockpit.v1");
            assert_eq!(*ttl_seconds, 86_400);
        }
        other => panic!("expected PauseKillBell, got {other:?}"),
    }
    assert!(matches!(
        app.action_pane.last_result.as_ref().map(|r| &r.outcome),
        Some(ActionOutcome::Submitted)
    ));
    // Mission strip mirror updates so operators see paused state.
    assert_eq!(app.delivery_snapshot.kill_bell_state, "paused");
}

#[tokio::test]
async fn adapter_error_surfaces_as_failed_outcome() {
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::with_error_on("post_passport_check");
    app.handle_delivery_action(DeliveryAction::ApproveOnce { pr_idx: 0 }, &fake)
        .await;
    let outcome = app
        .action_pane
        .last_result
        .as_ref()
        .map(|r| r.outcome.clone());
    match outcome {
        Some(ActionOutcome::Failed(msg)) => {
            assert!(msg.contains("post_passport_check"));
        }
        other => panic!("expected Failed outcome, got {other:?}"),
    }
    // No ledger row written on failure path.
    assert!(
        !call_names(&fake).contains(&"append_ledger"),
        "failed approve must not append a ledger row"
    );
}

#[tokio::test]
async fn approve_once_appends_ledger_entry_with_human_decision_kind() {
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(DeliveryAction::ApproveOnce { pr_idx: 0 }, &fake)
        .await;
    let kinds = ledger_kinds(&fake);
    assert_eq!(
        kinds,
        vec!["human_decision_recorded".to_string()],
        "approve must record exactly one HumanDecisionRecorded entry"
    );
    // Actor is the canonical TUI cockpit stamp.
    let actor_ok = fake.calls().into_iter().any(|c| match c {
        RecordedCall::AppendLedger { actor, .. } => actor == "tui.cockpit.v1",
        _ => false,
    });
    assert!(actor_ok, "ledger entry must be stamped 'tui.cockpit.v1'");
}

#[tokio::test]
async fn kill_bell_action_does_not_double_append_ledger_entry() {
    // The KillBell::pause path inside ProductionActionAdapter ALREADY
    // signs + appends a `KillBellEngaged` row; the handler MUST NOT
    // also call `adapter.append_ledger(KillBellEngaged{...})`. Verify
    // by inspecting the fake adapter's recorded calls.
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(
        DeliveryAction::KillBell {
            reason: "regression-fence".into(),
        },
        &fake,
    )
    .await;
    let kinds = ledger_kinds(&fake);
    assert!(
        kinds.is_empty(),
        "KillBell handler must defer ALL ledger append to the adapter; got {kinds:?}"
    );
    // And no extra calls besides the one pause.
    assert_eq!(fake.calls().len(), 1);
}

#[tokio::test]
async fn request_repair_with_no_reason_still_succeeds() {
    // RequestRepair takes no reason argument; ensure the handler
    // doesn't accidentally require one (edge case) and still posts a
    // valid comment.
    let mut app = app_with_demo_delivery().await;
    let fake = FakeActionAdapter::new();
    app.handle_delivery_action(DeliveryAction::RequestRepair { pr_idx: 0 }, &fake)
        .await;
    assert!(matches!(
        app.action_pane.last_result.as_ref().map(|r| &r.outcome),
        Some(ActionOutcome::Submitted)
    ));
    // Non-empty comment body even without explicit reason text.
    let has_nonempty_comment = fake.calls().into_iter().any(|c| {
        matches!(
            c,
            RecordedCall::PostMrComment { body, .. } if !body.is_empty()
        )
    });
    assert!(has_nonempty_comment, "repair must post a non-empty comment");
}
