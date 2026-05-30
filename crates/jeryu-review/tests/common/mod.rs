//! Shared fixtures for the integration tests (the crate-private `test_support`
//! module is not reachable from `tests/`).
//!
//! Each integration-test binary includes this module separately, so any helper
//! a given binary doesn't call reads as dead code there; allow it crate-wide.
#![allow(dead_code)]

use chrono::Utc;
use jeryu_review::schema::*;
use jeryu_review::signing::Signature;

pub fn assets_prompts_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

pub fn mint_pack() -> EvidencePack {
    pack_with(RiskTier::R2, true, ScanOutcome::Passed)
}

pub fn pack_with(tier: RiskTier, signed: bool, secret_scan: ScanOutcome) -> EvidencePack {
    EvidencePack {
        schema: SchemaTag::new(),
        id: "evp_it_test".into(),
        intent_id: None,
        repo: "org/proj".into(),
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
        prompt_sha: Some("sha256:abc".into()),
        provider: Some("canned".into()),
        model: Some("canned-model".into()),
        temperature: Some(0.0),
        seed: None,
        raw_response_sha: Some("sha256:def".into()),
        head_sha: pack.head_sha.clone(),
        policy_sha: pack.policy_sha.clone(),
        decision,
        reason: Some("canned".into()),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    }
}
