//! Evidence Pack builder.
//!
//! Composes an [`EvidencePack`] from the inputs available in a single PR
//! pipeline: change list, scanner outcomes, the proof/CI-lane receipt slice,
//! and a SHA-stamped evidence digest.

use crate::signing::sha256_digest;
use crate::types::{
    ChangedFile, CiCheck, EvidencePack, GateReceipt, RiskTier, RollbackSection, SchemaTag,
    SecuritySection, SupplyChainSection, TestsSection,
};
use chrono::{DateTime, Utc};

pub struct EvidenceInputs<'a> {
    pub repo: &'a str,
    pub source_branch: &'a str,
    pub target_branch: &'a str,
    pub head_sha: &'a str,
    pub base_sha: &'a str,
    pub policy_sha: &'a str,
    pub author_agent: Option<&'a str>,
    pub intent_id: Option<&'a str>,
    pub risk: RiskTier,
    pub changed_files: Vec<ChangedFile>,
    pub claims: Vec<String>,
    pub tests: TestsSection,
    pub security: SecuritySection,
    pub supply_chain: SupplyChainSection,
    pub rollback: RollbackSection,
    pub gate_receipts: Vec<GateReceipt>,
    /// Required-check / CI-lane status for the PR head. Defaults to empty via
    /// [`Default`]; gate-relevant only when the approvals policy declares
    /// `required_ci_lanes`.
    pub ci_status: Vec<CiCheck>,
}

pub fn build_evidence_pack(inp: EvidenceInputs<'_>) -> EvidencePack {
    let now = Utc::now();
    let id = mint_evp_id(now, inp.head_sha);
    // The evidence_digest is computed over a canonical projection of the pack
    // (everything except the digest itself and the signature). We sort
    // changed_files by path to make the digest stable across CI runs.
    let mut sorted_files = inp.changed_files;
    sorted_files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut pack = EvidencePack {
        schema: SchemaTag::new(),
        id,
        intent_id: inp.intent_id.map(|s| s.to_string()),
        repo: inp.repo.to_string(),
        source_branch: inp.source_branch.to_string(),
        target_branch: inp.target_branch.to_string(),
        head_sha: inp.head_sha.to_string(),
        base_sha: inp.base_sha.to_string(),
        policy_sha: inp.policy_sha.to_string(),
        author_agent: inp.author_agent.map(|s| s.to_string()),
        risk: inp.risk,
        changed_files: sorted_files,
        claims: inp.claims,
        tests: inp.tests,
        security: inp.security,
        supply_chain: inp.supply_chain,
        rollback: inp.rollback,
        gate_receipts: inp.gate_receipts,
        ci_status: inp.ci_status,
        evidence_digest: String::new(),
        created_at: now,
        signature: None,
    };
    let canonical = serialize_for_digest(&pack);
    pack.evidence_digest = sha256_digest(canonical.as_bytes());
    pack
}

/// Recompute the digest and compare to `pack.evidence_digest`.
pub fn verify_evidence_digest(pack: &EvidencePack) -> bool {
    let canonical = serialize_for_digest(pack);
    sha256_digest(canonical.as_bytes()) == pack.evidence_digest
}

/// Stable JSON projection: zero out the digest + signature so they don't
/// recursively perturb the hash.
fn serialize_for_digest(pack: &EvidencePack) -> String {
    let mut clone = pack.clone();
    clone.evidence_digest = String::new();
    clone.signature = None;
    serde_json::to_string(&clone).expect("evidence pack serialization")
}

fn mint_evp_id(now: DateTime<Utc>, head_sha: &str) -> String {
    let ts_hex = format!("{:013X}", now.timestamp_millis() as u64);
    let tail: String = head_sha
        .chars()
        .rev()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(13)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let mut s = format!("evp_{ts_hex}{tail}");
    while s.len() < 30 {
        s.push('0');
    }
    s.truncate(30);
    s
}

/// Build a [`GateReceipt`] from id/status/detail/evidence fields. A passing
/// proof / jeryu-ci lane maps to one of these.
pub fn make_gate_receipt(
    id: impl Into<String>,
    status: impl Into<String>,
    detail: impl Into<String>,
    evidence: Option<String>,
) -> GateReceipt {
    GateReceipt {
        id: id.into(),
        status: status.into(),
        detail: detail.into(),
        evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn minimal_inputs<'a>(head: &'a str, base: &'a str, policy: &'a str) -> EvidenceInputs<'a> {
        EvidenceInputs {
            repo: "org/proj",
            source_branch: "agent/x",
            target_branch: "main",
            head_sha: head,
            base_sha: base,
            policy_sha: policy,
            author_agent: Some("builder.x"),
            intent_id: None,
            risk: RiskTier::R2,
            changed_files: vec![
                ChangedFile {
                    path: "z.rs".into(),
                    risk_tags: vec![],
                    lines_added: 1,
                    lines_removed: 0,
                },
                ChangedFile {
                    path: "a.rs".into(),
                    risk_tags: vec![],
                    lines_added: 1,
                    lines_removed: 0,
                },
            ],
            claims: vec!["fix bug".into()],
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
            ci_status: vec![],
        }
    }

    #[test]
    fn pack_digest_is_stable_under_file_order() {
        let (h, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
        let p1 = build_evidence_pack(minimal_inputs(&h, &b, &c));
        let p2 = build_evidence_pack(minimal_inputs(&h, &b, &c));
        let mut p1c = p1.clone();
        let p2c = p2.clone();
        p1c.created_at = p2c.created_at;
        p1c.id = p2c.id.clone();
        let p1c = build_pack_with_fixed_now(p1c);
        let p2c = build_pack_with_fixed_now(p2c);
        assert_eq!(p1c.evidence_digest, p2c.evidence_digest);
    }

    fn build_pack_with_fixed_now(mut p: EvidencePack) -> EvidencePack {
        let canonical = serialize_for_digest(&p);
        p.evidence_digest = sha256_digest(canonical.as_bytes());
        p
    }

    #[test]
    fn verify_evidence_digest_round_trips() {
        let (h, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
        let p = build_evidence_pack(minimal_inputs(&h, &b, &c));
        assert!(verify_evidence_digest(&p));
        let mut tampered = p.clone();
        tampered.repo = "tampered".into();
        assert!(!verify_evidence_digest(&tampered));
    }

    #[test]
    fn gate_receipt_helper_round_trips() {
        let r1 = make_gate_receipt("intake", "pass", "ok", Some("/tmp/x.json".into()));
        let r2 = make_gate_receipt("vti-plan", "pending", "awaiting CI", None);
        assert_eq!(r1.id, "intake");
        assert_eq!(r1.status, "pass");
        assert_eq!(r1.evidence.as_deref(), Some("/tmp/x.json"));
        assert_eq!(r2.status, "pending");
        assert!(r2.evidence.is_none());
    }

    #[test]
    fn changed_files_sorted_in_pack() {
        let (h, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
        let p = build_evidence_pack(minimal_inputs(&h, &b, &c));
        let paths: Vec<&str> = p.changed_files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.rs", "z.rs"]);
    }
}
