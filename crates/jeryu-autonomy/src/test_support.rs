//! Shared unit-test fixtures for the autonomy crate.
//!
//! Both the judge and full-auto suites drive the pure judge against a derived
//! bundle, so the `EvidencePack` builder and the default bundle live here once
//! rather than being copied into each `tests` module (clears HLT-043 copy-code
//! and the `severe-duplication-in-product-code` cap).

use crate::evidence::{EvidenceInputs, build_evidence_pack};
use crate::policy_yaml::{PolicyBundle, fixtures};
use crate::signing::Signature;
use crate::types::*;

/// The default policy bundle shared by the unit suites.
pub(crate) fn bundle() -> PolicyBundle {
    fixtures::default_bundle()
}

/// Build an `EvidencePack` at `tier`, optionally signed, optionally with a
/// failed secret scan. Shared by the judge and full-auto suites.
pub(crate) fn pack_at_tier(tier: RiskTier, signed: bool, secret_failed: bool) -> EvidencePack {
    let (h, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
    let mut p = build_evidence_pack(EvidenceInputs {
        repo: "org/p",
        source_branch: "agent/x",
        target_branch: "main",
        head_sha: &h,
        base_sha: &b,
        policy_sha: &c,
        author_agent: Some("builder.x"),
        intent_id: None,
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
            secret_scan: if secret_failed {
                ScanOutcome::Failed
            } else {
                ScanOutcome::Passed
            },
        },
        supply_chain: SupplyChainSection::default(),
        rollback: RollbackSection {
            strategy: RollbackStrategy::RevertCommit,
            feature_flag: None,
            data_migration_reversible: Some(true),
        },
        gate_receipts: vec![],
        ci_status: vec![],
    });
    if signed {
        p.signature = Some(Signature {
            key_id: "evidence-builder.v1".into(),
            algo: "ed25519".into(),
            value: "0".repeat(128),
        });
    }
    p
}
