use super::*;
use crate::conditions::HardStop;
use crate::schema::{ReviewDecision, ReviewerRole, RiskTier, ScanOutcome};
use crate::test_support::{pack_with, receipt_for};

fn bundle() -> PolicyBundle {
    PolicyBundle::default_enforcing()
}

#[test]
fn allow_merge_when_quorum_met_no_hard_stops() {
    let b = bundle();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let receipts = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
        policy: &b,
        repo: "org/p",
        target_branch: "main",
        pull_request: Some("pr-1"),
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::AllowMerge);
    assert!(out.verdict.hard_stops.is_empty());
    assert_eq!(out.dropped_receipts.len(), 0);
}

#[test]
fn one_blocking_reviewer_rejects_via_hard_stop() {
    let b = bundle();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let receipts = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Block, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs::new(&p, &receipts, &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .contains(&"reviewer_blocked".to_string())
    );
}

#[test]
fn secret_scan_failure_rejects_even_with_unanimous_approval() {
    let b = bundle();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Failed);
    let receipts = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs::new(&p, &receipts, &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "secret_scan_failed")
    );
}

#[test]
fn sha_drift_drops_receipt() {
    let b = bundle();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let mut bad = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
    bad.head_sha = "d".repeat(40);
    let good = receipt_for(
        ReviewerRole::TestIntegrity,
        "test.v1",
        ReviewDecision::Pass,
        &p,
    );
    let receipts = vec![bad, good];
    let out = judge(JudgeInputs::new(&p, &receipts, &b, "org/p", "main"));
    // Drift drops the security receipt → missing role → require_human.
    assert_eq!(out.dropped_receipts.len(), 1);
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
}

#[test]
fn unsigned_pack_fails_closed_via_evidence_signature_invalid() {
    let b = bundle();
    let p = pack_with(RiskTier::R2, false, ScanOutcome::Passed); // NOT signed
    let receipts = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let out = judge(JudgeInputs::new(&p, &receipts, &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::Reject);
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "evidence_signature_invalid")
    );
}

#[test]
fn injected_codeowners_not_satisfied_forces_reject() {
    let b = bundle();
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let receipts = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    let injected = [HardStop {
        name: "codeowners_not_satisfied".into(),
        reason: "no @security approval on src/auth/login.rs".into(),
        details: serde_json::json!({"path": "src/auth/login.rs"}),
    }];
    let out = judge(JudgeInputs {
        pack: &p,
        receipts: &receipts,
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
fn r4_protected_path_requires_human_even_with_all_passes() {
    let b = bundle();
    let p = pack_with(RiskTier::R4, true, ScanOutcome::Passed);
    // R4 quorum has human_required=true → RequireHuman even with no receipts.
    let out = judge(JudgeInputs::new(&p, &[], &b, "org/p", "main"));
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
}

#[test]
fn mint_verdict_id_is_30_chars_and_prefixed() {
    let now = Utc::now();
    let id = mint_verdict_id(now, &"f".repeat(40));
    assert!(id.starts_with("vgv_"));
    assert_eq!(id.len(), 30);
    let id2 = mint_verdict_id(now, "abc");
    assert!(id2.starts_with("vgv_"));
    assert_eq!(id2.len(), 30);
}
