//! Exact-SHA binding (Tip1 Law 4).
//!
//! A receipt or verdict is valid only against a specific (head_sha, policy_sha)
//! tuple. Any drift invalidates the receipt/verdict — the orchestrator must
//! re-run reviews or fail closed.

use crate::autonomy::types::{AgentApprovalReceipt, EvidencePack};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShaBindError {
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
    #[error("evidence_pack_id mismatch: receipt says {receipt_id}, pack says {pack_id}")]
    PackIdMismatch { receipt_id: String, pack_id: String },
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
    use crate::autonomy::evidence::{EvidenceInputs, build_evidence_pack};
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::*;
    use chrono::Utc;

    fn pack() -> EvidencePack {
        build_evidence_pack(EvidenceInputs {
            repo: "org/p",
            source_branch: "agent/x",
            target_branch: "main",
            head_sha: "a".repeat(40).leak(),
            base_sha: "b".repeat(40).leak(),
            policy_sha: "c".repeat(40).leak(),
            author_agent: Some("builder"),
            intent_id: None,
            risk: RiskTier::R2,
            changed_files: vec![],
            claims: vec![],
            tests: TestsSection {
                targeted: vec![],
                full_required: false,
                skipped: vec![],
                coverage_delta: None,
            },
            security: SecuritySection {
                sast: ScanOutcome::Passed,
                dependency_scan: ScanOutcome::Passed,
                secret_scan: ScanOutcome::Passed,
            },
            supply_chain: SupplyChainSection::default(),
            rollback: RollbackSection {
                strategy: RollbackStrategy::RevertCommit,
                feature_flag: None,
                data_migration_reversible: Some(true),
            },
            gate_receipts: vec![],
        })
    }

    fn receipt_for(p: &EvidencePack) -> AgentApprovalReceipt {
        AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: "aar_x".into(),
            evidence_pack_id: p.id.clone(),
            role: ReviewerRole::Security,
            agent_id: "sec.v1".into(),
            prompt_sha: None,
            provider: None,
            model: None,
            temperature: None,
            seed: None,
            raw_response_sha: None,
            head_sha: p.head_sha.clone(),
            policy_sha: p.policy_sha.clone(),
            decision: ReviewDecision::Pass,
            reason: None,
            findings: vec![],
            not_author: true,
            tokens: TokenCounts::default(),
            created_at: Utc::now(),
            signature: Signature::stub(),
        }
    }

    #[test]
    fn matching_sha_passes() {
        let p = pack();
        let r = receipt_for(&p);
        assert!(verify_sha_binding(&p, &r).is_ok());
    }

    #[test]
    fn head_drift_rejects() {
        let p = pack();
        let mut r = receipt_for(&p);
        r.head_sha = "d".repeat(40);
        let err = verify_sha_binding(&p, &r).unwrap_err();
        assert!(matches!(err, ShaBindError::HeadMismatch { .. }));
    }

    #[test]
    fn policy_drift_rejects() {
        let p = pack();
        let mut r = receipt_for(&p);
        r.policy_sha = "e".repeat(40);
        let err = verify_sha_binding(&p, &r).unwrap_err();
        assert!(matches!(err, ShaBindError::PolicyMismatch { .. }));
    }
}
