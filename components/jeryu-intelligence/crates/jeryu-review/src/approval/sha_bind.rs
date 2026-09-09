//! Exact-SHA binding (Law 4).
//!
//! A receipt is valid only against a specific
//! `(evidence_pack_id, head_sha, policy_sha)` tuple. Any drift invalidates the
//! receipt — the orchestrator must re-run reviews or fail closed.

use crate::schema::{AgentApprovalReceipt, EvidencePack};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShaBindError {
    #[error("evidence_pack_id mismatch: receipt says {receipt_id}, pack says {pack_id}")]
    PackIdMismatch { receipt_id: String, pack_id: String },
    #[error("head_sha mismatch: receipt says {receipt_head}, pack says {pack_head}")]
    HeadMismatch {
        receipt_head: String,
        pack_head: String,
    },
    #[error("policy_sha mismatch: receipt says {receipt_policy}, pack says {pack_policy}")]
    PolicyMismatch {
        receipt_policy: String,
        pack_policy: String,
    },
}

pub fn verify_sha_binding(
    pack: &EvidencePack,
    receipt: &AgentApprovalReceipt,
) -> Result<(), ShaBindError> {
    if receipt.evidence_pack_id != pack.id {
        return Err(ShaBindError::PackIdMismatch {
            receipt_id: receipt.evidence_pack_id.clone(),
            pack_id: pack.id.clone(),
        });
    }
    if receipt.head_sha != pack.head_sha {
        return Err(ShaBindError::HeadMismatch {
            receipt_head: receipt.head_sha.clone(),
            pack_head: pack.head_sha.clone(),
        });
    }
    if receipt.policy_sha != pack.policy_sha {
        return Err(ShaBindError::PolicyMismatch {
            receipt_policy: receipt.policy_sha.clone(),
            pack_policy: pack.policy_sha.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ReviewDecision, ReviewerRole, RiskTier, ScanOutcome};
    use crate::test_support::{pack_with, receipt_for};

    #[test]
    fn matching_sha_passes() {
        let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
        let r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
        assert!(verify_sha_binding(&p, &r).is_ok());
    }

    #[test]
    fn head_drift_rejects() {
        let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
        let mut r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
        r.head_sha = "d".repeat(40);
        assert!(matches!(
            verify_sha_binding(&p, &r).unwrap_err(),
            ShaBindError::HeadMismatch { .. }
        ));
    }

    #[test]
    fn policy_drift_rejects() {
        let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
        let mut r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
        r.policy_sha = "e".repeat(40);
        assert!(matches!(
            verify_sha_binding(&p, &r).unwrap_err(),
            ShaBindError::PolicyMismatch { .. }
        ));
    }

    #[test]
    fn pack_id_drift_rejects() {
        let p = pack_with(RiskTier::R2, true, ScanOutcome::Passed);
        let mut r = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
        r.evidence_pack_id = "evp_other".into();
        assert!(matches!(
            verify_sha_binding(&p, &r).unwrap_err(),
            ShaBindError::PackIdMismatch { .. }
        ));
    }
}
