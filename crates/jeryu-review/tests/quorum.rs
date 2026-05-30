//! Quorum voting via the public API: required-role pass, veto, self-approval.

mod common;

use common::{pack_with, receipt_for};
use jeryu_review::approval::{QuorumDecision, evaluate_quorum};
use jeryu_review::policy::PolicyBundle;
use jeryu_review::schema::{ReviewDecision, ReviewerRole, RiskTier, ScanOutcome};

fn r2_policy() -> jeryu_review::policy::ApprovalsPolicy {
    PolicyBundle::default_enforcing().approvals
}

#[test]
fn quorum_met_when_required_roles_pass() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
    ];
    assert_eq!(
        evaluate_quorum(RiskTier::R2, &r, &r2_policy(), Some("builder.x")).decision,
        QuorumDecision::Met
    );
}

#[test]
fn one_block_vetoes_regardless_of_count() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Block, &p),
        receipt_for(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        ),
        receipt_for(ReviewerRole::Runtime, "rt.v1", ReviewDecision::Pass, &p),
    ];
    let out = evaluate_quorum(RiskTier::R2, &r, &r2_policy(), None);
    assert_eq!(out.decision, QuorumDecision::Vetoed);
    assert_eq!(out.blocking_roles, vec![ReviewerRole::Security]);
}

#[test]
fn missing_required_role_is_insufficient() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = vec![receipt_for(
        ReviewerRole::Security,
        "sec.v1",
        ReviewDecision::Pass,
        &p,
    )];
    let out = evaluate_quorum(RiskTier::R2, &r, &r2_policy(), None);
    assert_eq!(out.decision, QuorumDecision::Insufficient);
    assert!(out.missing_roles.contains(&ReviewerRole::TestIntegrity));
}

#[test]
fn author_self_approval_does_not_count() {
    let p = pack_with(RiskTier::R1, true, ScanOutcome::Passed);
    let mut self_r = receipt_for(
        ReviewerRole::TestIntegrity,
        "builder.author",
        ReviewDecision::Pass,
        &p,
    );
    self_r.not_author = false;
    assert_eq!(
        evaluate_quorum(
            RiskTier::R1,
            &[self_r],
            &r2_policy(),
            Some("builder.author")
        )
        .decision,
        QuorumDecision::Insufficient
    );
}

#[test]
fn human_required_lands_separately() {
    let p = pack_with(RiskTier::R4, true, ScanOutcome::Passed);
    assert_eq!(
        evaluate_quorum(RiskTier::R4, &[], &r2_policy(), None).decision,
        QuorumDecision::HumanRequired
    );
    let _ = p;
}

#[test]
fn author_identity_overrides_lying_not_author_flag() {
    let p = pack_with(RiskTier::R1, true, ScanOutcome::Passed);
    let mut r = receipt_for(
        ReviewerRole::TestIntegrity,
        "builder.author",
        ReviewDecision::Pass,
        &p,
    );
    r.not_author = true; // lies
    assert_eq!(
        evaluate_quorum(RiskTier::R1, &[r], &r2_policy(), Some("builder.author")).decision,
        QuorumDecision::Insufficient
    );
}

#[test]
fn duplicate_agent_identities_collapse() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = vec![
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
        receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
    ];
    assert_eq!(
        evaluate_quorum(RiskTier::R2, &r, &r2_policy(), None).decision,
        QuorumDecision::Insufficient
    );
}
