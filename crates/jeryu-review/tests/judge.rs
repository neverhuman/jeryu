//! Judge policy-fusion via the public API: SHA-bind → hard-stops → quorum.

mod common;

use common::{pack_with, receipt_for};
use jeryu_review::conditions::HardStop;
use jeryu_review::judge::{JudgeInputs, judge, mint_verdict_id};
use jeryu_review::policy::PolicyBundle;
use jeryu_review::schema::{GateDecision, ReviewDecision, ReviewerRole, RiskTier, ScanOutcome};

fn passes(
    p: &jeryu_review::schema::EvidencePack,
) -> Vec<jeryu_review::schema::AgentApprovalReceipt> {
    vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            p,
        ),
    ]
}

#[test]
fn allow_merge_when_quorum_met_no_hard_stops() {
    let b = PolicyBundle::default_enforcing();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = passes(&p);
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &r,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: Some("pr-1"),
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::AllowMerge);
}

#[test]
fn one_block_rejects_via_reviewer_blocked() {
    let b = PolicyBundle::default_enforcing();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Block, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs::new(&p, &r, &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "reviewer_blocked")
    );
}

#[test]
fn secret_scan_failed_rejects_with_unanimous_approval() {
    let b = PolicyBundle::default_enforcing();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Failed);
    let r = passes(&p);
    let out = judge(JudgeInputs::new(&p, &r, &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "secret_scan_failed")
    );
}

#[test]
fn sha_drift_drops_receipt_and_requires_human() {
    let b = PolicyBundle::default_enforcing();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let mut bad = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
    bad.head_sha = "d".repeat(40);
    let good = receipt_for(
        ReviewerRole::TestIntegrity,
        "test.v1",
        ReviewDecision::Pass,
        &p,
    );
    let out = judge(JudgeInputs::new(&p, &[bad, good], &b, "org/p", "main"));
    assert_eq!(out.dropped_receipts.len(), 1);
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
}

#[test]
fn unsigned_pack_fails_closed() {
    let b = PolicyBundle::default_enforcing();
    let p = pack_with(RiskTier::R2, false, ScanOutcome::Passed);
    let r = passes(&p);
    let out = judge(JudgeInputs::new(&p, &r, &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "evidence_signature_invalid")
    );
}

#[test]
fn injected_codeowners_forces_reject() {
    let b = PolicyBundle::default_enforcing();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = passes(&p);
    let injected = [HardStop {
        name: "codeowners_not_satisfied".into(),
        reason: "no owner approval".into(),
        details: serde_json::Value::Null,
    }];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &r,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: None,
        author_agent: None,
        external_hard_stops: &injected,
    });
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "codeowners_not_satisfied")
    );
}

#[test]
fn r4_requires_human_even_with_passes() {
    let b = PolicyBundle::default_enforcing();
    let p = pack_with(RiskTier::R4, true, ScanOutcome::Passed);
    let out = judge(JudgeInputs::new(&p, &[], &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
}

#[test]
fn verdict_id_is_30_chars_and_prefixed() {
    let id = mint_verdict_id(chrono::Utc::now(), &"f".repeat(40));
    assert!(id.starts_with("vgv_"));
    assert_eq!(id.len(), 30);
}
