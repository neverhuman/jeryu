//! Shared in-crate test fixtures: minting evidence packs and receipts so the
//! unit tests in `conditions`, `judge`, `quorum`, `sha_bind`, and `rejudge`
//! agree on one canonical builder.
//!
//! Compiled only under `cfg(test)`. The integration tests in `tests/` build
//! their own equivalents because they cannot reach this private module.

#![cfg(test)]

use crate::schema::{
    AgentApprovalReceipt, EvidencePack, ReviewDecision, ReviewerRole, RiskTier, RollbackSection,
    RollbackStrategy, ScanOutcome, SchemaTag, SecuritySection, SupplyChainSection, TestsSection,
    TokenCounts,
};
use crate::signing::Signature;
use chrono::Utc;

/// Mint an evidence pack at a risk tier. `signed` controls whether the pack
/// carries an ed25519 signature (so `evidence_signature_invalid` accepts it);
/// `secret_scan` sets the secret-scan outcome.
pub fn pack_with(tier: RiskTier, signed: bool, secret_scan: ScanOutcome) -> EvidencePack {
    EvidencePack {
        schema: SchemaTag::new(),
        id: "evp_test".into(),
        intent_id: None,
        repo: "org/p".into(),
        source_branch: "agent/x".into(),
        target_branch: "main".into(),
        head_sha: "a".repeat(40),
        base_sha: "b".repeat(40),
        policy_sha: "c".repeat(40),
        author_agent: Some("builder.x".into()),
        risk: tier,
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
            secret_scan,
        },
        supply_chain: SupplyChainSection::default(),
        rollback: RollbackSection {
            strategy: RollbackStrategy::RevertCommit,
            feature_flag: None,
            data_migration_reversible: Some(true),
        },
        gate_receipts: vec![],
        evidence_digest: format!("sha256:{}", "0".repeat(64)),
        created_at: Utc::now(),
        signature: signed.then(|| Signature {
            key_id: "evidence-builder.v1".into(),
            algo: "ed25519".into(),
            value: "0".repeat(128),
        }),
    }
}

/// Build a receipt bound to `pack`'s SHA tuple.
pub fn receipt_for(
    role: ReviewerRole,
    agent: &str,
    decision: ReviewDecision,
    pack: &EvidencePack,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: format!("aar_{agent}"),
        evidence_pack_id: pack.id.clone(),
        role,
        agent_id: agent.into(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: Some("sha256:beef".into()),
        head_sha: pack.head_sha.clone(),
        policy_sha: pack.policy_sha.clone(),
        decision,
        reason: None,
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    }
}
