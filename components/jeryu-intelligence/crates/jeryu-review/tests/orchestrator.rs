//! Orchestrator integration tests (ports the 14 source cases): Fake double
//! behavior + Production orchestrator budget/abstain/signing invariants.

mod common;

use common::{assets_prompts_dir, mint_pack, receipt_for};
use jeryu_review::llm::{Budget, BudgetLedger, LlmRouter, TokenUsage};
use jeryu_review::orchestrator::{
    FakeReviewerOrchestrator, ProductionReviewerOrchestrator, ReviewerOrchestrator,
};
use jeryu_review::schema::{ReviewDecision, ReviewerRole};
use jeryu_review::signing::{EdSigningKey, Signature};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

// ---- 1. Fake returns canned receipts ---------------------------------------

#[tokio::test]
async fn fake_orchestrator_returns_canned_receipts() {
    let pack = mint_pack();
    let orch = FakeReviewerOrchestrator::new().with_canned(
        ReviewerRole::Security,
        receipt_for(
            ReviewerRole::Security,
            "sec.v1",
            ReviewDecision::Block,
            &pack,
        ),
    );
    let out = orch
        .run_all(&pack, &[ReviewerRole::Security], "diff")
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, ReviewerRole::Security);
    assert!(matches!(out[0].decision, ReviewDecision::Block));
}

// ---- 2. Fake records each required role ------------------------------------

#[tokio::test]
async fn fake_orchestrator_records_each_required_role() {
    let orch = FakeReviewerOrchestrator::new();
    let pack = mint_pack();
    let roles = vec![
        ReviewerRole::Security,
        ReviewerRole::TestIntegrity,
        ReviewerRole::Runtime,
        ReviewerRole::Lockfile,
    ];
    let _ = orch.run_all(&pack, &roles, "diff").await.unwrap();
    let mut recorded = orch.recorded_calls.lock().unwrap().clone();
    recorded.sort_by_key(|r| format!("{r:?}"));
    let mut expected = roles.clone();
    expected.sort_by_key(|r| format!("{r:?}"));
    assert_eq!(recorded, expected);
}

// ---- 3. error_on returns abstain for that role only ------------------------

#[tokio::test]
async fn fake_orchestrator_error_on_returns_abstain_for_that_role() {
    let pack = mint_pack();
    let orch = FakeReviewerOrchestrator::new()
        .with_canned(
            ReviewerRole::TestIntegrity,
            receipt_for(
                ReviewerRole::TestIntegrity,
                "test.v1",
                ReviewDecision::Pass,
                &pack,
            ),
        )
        .error_on(ReviewerRole::TestIntegrity);
    let out = orch
        .run_all(
            &pack,
            &[ReviewerRole::TestIntegrity, ReviewerRole::Runtime],
            "diff",
        )
        .await
        .unwrap();
    let ti = out
        .iter()
        .find(|r| r.role == ReviewerRole::TestIntegrity)
        .unwrap();
    assert!(matches!(ti.decision, ReviewDecision::Abstain));
    let rt = out
        .iter()
        .find(|r| r.role == ReviewerRole::Runtime)
        .unwrap();
    assert!(matches!(rt.decision, ReviewDecision::Pass));
}

// ---- 4. Unknown role returns default Pass receipt --------------------------

#[tokio::test]
async fn fake_orchestrator_unknown_role_returns_default_pass_receipt() {
    let orch = FakeReviewerOrchestrator::new();
    let pack = mint_pack();
    let out = orch
        .run_all(&pack, &[ReviewerRole::Lockfile], "diff")
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, ReviewerRole::Lockfile);
    assert!(matches!(out[0].decision, ReviewDecision::Pass));
    assert_eq!(out[0].agent_id, "reviewer-lockfile.v1");
}

// ---- 5. One receipt per required role --------------------------------------

#[tokio::test]
async fn run_all_returns_one_receipt_per_required_role() {
    let orch = FakeReviewerOrchestrator::new();
    let pack = mint_pack();
    let roles = vec![
        ReviewerRole::Security,
        ReviewerRole::TestIntegrity,
        ReviewerRole::Runtime,
        ReviewerRole::Lockfile,
    ];
    let out = orch.run_all(&pack, &roles, "diff").await.unwrap();
    assert_eq!(out.len(), roles.len());
}

// ---- 6. Empty required_roles → empty Vec -----------------------------------

#[tokio::test]
async fn run_all_empty_required_roles_returns_empty_vec() {
    let orch = FakeReviewerOrchestrator::new();
    let pack = mint_pack();
    assert!(orch.run_all(&pack, &[], "diff").await.unwrap().is_empty());
}

// ---- 7. Concurrent reviewers complete in parallel --------------------------

#[tokio::test]
async fn run_all_with_concurrent_roles_completes_all_in_parallel() {
    let orch = FakeReviewerOrchestrator::new()
        .with_latency(ReviewerRole::Security, 50)
        .with_latency(ReviewerRole::TestIntegrity, 50)
        .with_latency(ReviewerRole::Runtime, 50)
        .with_latency(ReviewerRole::Lockfile, 50);
    let pack = mint_pack();
    let roles = vec![
        ReviewerRole::Security,
        ReviewerRole::TestIntegrity,
        ReviewerRole::Runtime,
        ReviewerRole::Lockfile,
    ];
    let started = Instant::now();
    let out = orch.run_all(&pack, &roles, "diff").await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(out.len(), 4);
    assert!(
        elapsed.as_millis() < 200,
        "4 x 50ms reviewers should run concurrently; took {elapsed:?}"
    );
}

fn exhausted_orchestrator(seed: u8) -> ProductionReviewerOrchestrator {
    let ledger = Arc::new(BudgetLedger::new());
    ledger.record(TokenUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        estimated_micro_usd: 10_000_000,
    });
    let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [seed; 32]));
    let router = Arc::new(LlmRouter::new());
    ProductionReviewerOrchestrator::new(router, ledger, PathBuf::from("/tmp/does-not-exist"), key)
        .with_budget(Budget {
            daily_micro_usd_cap: 1_000,
            per_pr_micro_usd_cap: 500,
        })
}

// ---- 8. Production + exhausted budget → all abstain ------------------------

#[tokio::test]
async fn production_orchestrator_with_exhausted_budget_returns_abstain_for_all_roles() {
    let orch = exhausted_orchestrator(1);
    let pack = mint_pack();
    let roles = vec![
        ReviewerRole::Security,
        ReviewerRole::TestIntegrity,
        ReviewerRole::Runtime,
        ReviewerRole::Lockfile,
    ];
    let out = orch.run_all(&pack, &roles, "diff").await.unwrap();
    assert_eq!(out.len(), 4);
    for r in &out {
        assert!(matches!(r.decision, ReviewDecision::Abstain));
        assert!(
            r.reason
                .as_deref()
                .unwrap_or("")
                .contains("budget exhausted")
        );
    }
}

// ---- 9. Production orchestrator constructs ---------------------------------

#[tokio::test]
async fn production_orchestrator_construct_with_required_fields() {
    let router = Arc::new(LlmRouter::new());
    let ledger = Arc::new(BudgetLedger::new());
    let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [2u8; 32]));
    let orch = ProductionReviewerOrchestrator::new(
        router.clone(),
        ledger.clone(),
        assets_prompts_dir(),
        key.clone(),
    );
    assert!(Arc::ptr_eq(&orch.router, &router));
    assert!(Arc::ptr_eq(&orch.budget_ledger, &ledger));
    assert!(Arc::ptr_eq(&orch.signing_key, &key));
    assert_eq!(orch.autonomy_dir, assets_prompts_dir());
}

// ---- 10. Synthesized abstain carries correct role --------------------------

#[tokio::test]
async fn abstain_receipt_for_role_carries_role_field() {
    let orch = exhausted_orchestrator(3);
    let pack = mint_pack();
    for role in [
        ReviewerRole::Security,
        ReviewerRole::TestIntegrity,
        ReviewerRole::Runtime,
        ReviewerRole::Lockfile,
    ] {
        let out = orch.run_all(&pack, &[role], "diff").await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, role);
    }
}

// ---- 11. Abstain receipt signature verifies under ed25519 ------------------

#[tokio::test]
async fn abstain_receipt_for_role_signature_is_valid_ed25519() {
    let ledger = Arc::new(BudgetLedger::new());
    ledger.record(TokenUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        estimated_micro_usd: 10_000_000,
    });
    let key = Arc::new(EdSigningKey::from_seed("orchestrator.test", [4u8; 32]));
    let verifier = key.verifier();
    let router = Arc::new(LlmRouter::new());
    let orch = ProductionReviewerOrchestrator::new(
        router,
        ledger,
        PathBuf::from("/tmp/does-not-exist"),
        key,
    )
    .with_budget(Budget {
        daily_micro_usd_cap: 1_000,
        per_pr_micro_usd_cap: 500,
    });
    let pack = mint_pack();
    let out = orch
        .run_all(&pack, &[ReviewerRole::Security], "diff")
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
    let r = &out[0];
    assert_eq!(r.signature.algo, "ed25519");
    let mut clone = r.clone();
    clone.signature = Signature::unsigned();
    let body = serde_json::to_string(&clone).unwrap();
    assert!(
        verifier.verify(body.as_bytes(), &r.signature),
        "abstain receipt signature must verify under the orchestrator's ed25519 key"
    );
}

// ---- 12. Receipt evidence_pack_id matches input pack -----------------------

#[tokio::test]
async fn receipt_evidence_pack_id_matches_input_pack() {
    let orch = exhausted_orchestrator(5);
    let pack = mint_pack();
    let out = orch
        .run_all(
            &pack,
            &[ReviewerRole::Security, ReviewerRole::TestIntegrity],
            "diff",
        )
        .await
        .unwrap();
    for r in &out {
        assert_eq!(r.evidence_pack_id, pack.id);
    }
}

// ---- 13. Receipt SHAs match pack so judge accepts them ---------------------

#[tokio::test]
async fn receipt_head_sha_and_policy_sha_match_pack() {
    let orch = exhausted_orchestrator(6);
    let pack = mint_pack();
    let out = orch
        .run_all(
            &pack,
            &[
                ReviewerRole::Security,
                ReviewerRole::TestIntegrity,
                ReviewerRole::Runtime,
                ReviewerRole::Lockfile,
            ],
            "diff",
        )
        .await
        .unwrap();
    for r in &out {
        assert_eq!(r.head_sha, pack.head_sha);
        assert_eq!(r.policy_sha, pack.policy_sha);
    }
}

// ---- 14. not_author is true on all synthesized receipts --------------------

#[tokio::test]
async fn not_author_flag_is_true_on_all_synthesized_receipts() {
    let orch = exhausted_orchestrator(7);
    let pack = mint_pack();
    let out = orch
        .run_all(
            &pack,
            &[
                ReviewerRole::Security,
                ReviewerRole::TestIntegrity,
                ReviewerRole::Runtime,
                ReviewerRole::Lockfile,
            ],
            "diff",
        )
        .await
        .unwrap();
    for r in &out {
        assert!(
            r.not_author,
            "synthesized receipt for {:?} must set not_author",
            r.role
        );
    }
}
