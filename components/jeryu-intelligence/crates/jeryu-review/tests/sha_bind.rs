//! SHA-bind verification via the public API (Law 4): a receipt is valid only
//! for one (evidence_pack_id, head_sha, policy_sha) tuple.

mod common;

use common::{pack_with, receipt_for};
use jeryu_review::approval::{ShaBindError, verify_sha_binding};
use jeryu_review::schema::{ReviewDecision, ReviewerRole, RiskTier, ScanOutcome};

#[test]
fn matching_sha_passes() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
    assert!(verify_sha_binding(&p, &r).is_ok());
}

#[test]
fn head_drift_rejected() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let mut r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
    r.head_sha = "d".repeat(40);
    assert!(matches!(
        verify_sha_binding(&p, &r).unwrap_err(),
        ShaBindError::HeadMismatch { .. }
    ));
}

#[test]
fn policy_drift_rejected() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let mut r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
    r.policy_sha = "e".repeat(40);
    assert!(matches!(
        verify_sha_binding(&p, &r).unwrap_err(),
        ShaBindError::PolicyMismatch { .. }
    ));
}

#[test]
fn pack_id_mismatch_rejected() {
    let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
    let mut r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
    r.evidence_pack_id = "evp_other".into();
    assert!(matches!(
        verify_sha_binding(&p, &r).unwrap_err(),
        ShaBindError::PackIdMismatch { .. }
    ));
}
