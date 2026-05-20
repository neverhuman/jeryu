//! Judge agent — pure policy fusion.
//!
//! Takes an EvidencePack, a set of signed AgentApprovalReceipts, and the
//! PolicyBundle loaded from the *target branch* (Tip1 Law 3). Emits a signed
//! VibeGateVerdict. **The judge never reads code** — eliminating the LLM
//! attack surface for the fusion step.
//!
//! Order of operations:
//!   1. SHA-bind every receipt to the pack. Receipts with drift → drop, log.
//!   2. Walk approvals policy `hard_stops` through the conditions registry.
//!      ANY hit → `Reject`. (Veto > approval count.)
//!   3. Evaluate quorum for the pack's risk tier.
//!   4. If `HumanRequired` → `RequireHuman`. Else → `AllowMerge`.

use crate::approval::quorum::{QuorumDecision, evaluate_quorum};
use crate::approval::sha_bind::verify_sha_binding;
use crate::autonomy::conditions::{ConditionRegistry, HardStop};
use crate::autonomy::policy_yaml::PolicyBundle;
use crate::autonomy::signing::Signature;
use crate::autonomy::types::{
    AgentApprovalReceipt, EvidencePack, GateDecision, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
};
use chrono::{Duration, Utc};

pub struct JudgeInputs<'a> {
    pub pack: &'a EvidencePack,
    pub receipts: &'a [AgentApprovalReceipt],
    pub policy: &'a PolicyBundle,
    pub repo: &'a str,
    pub target_branch: &'a str,
    pub merge_request: Option<&'a str>,
    pub author_agent: Option<&'a str>,
    /// Hard stops the orchestrator pre-computed (e.g. `codeowners_not_satisfied`,
    /// `freeze_window_active`, `budget_exceeded`). Merged with registry-computed
    /// hits; ANY hit → Reject (veto > approval).
    #[doc(hidden)]
    pub external_hard_stops: &'a [HardStop],
}

impl<'a> JudgeInputs<'a> {
    /// Convenience constructor with no externally-injected hard stops.
    pub fn new(
        pack: &'a EvidencePack,
        receipts: &'a [AgentApprovalReceipt],
        policy: &'a PolicyBundle,
        repo: &'a str,
        target_branch: &'a str,
    ) -> Self {
        Self {
            pack,
            receipts,
            policy,
            repo,
            target_branch,
            merge_request: None,
            author_agent: None,
            external_hard_stops: &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct JudgeOutcome {
    pub verdict: VibeGateVerdict,
    /// Receipts that failed SHA binding; not included in the verdict.
    pub dropped_receipts: Vec<String>,
}

pub fn judge(inputs: JudgeInputs<'_>) -> JudgeOutcome {
    // 1. SHA-bind filter.
    let mut bound: Vec<&AgentApprovalReceipt> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    for r in inputs.receipts {
        if verify_sha_binding(inputs.pack, r).is_ok() {
            bound.push(r);
        } else {
            dropped.push(r.id.clone());
        }
    }
    let bound_owned: Vec<AgentApprovalReceipt> = bound.iter().map(|r| (*r).clone()).collect();

    // 2. Hard stops. Merge registry-computed with caller-injected.
    let registry = ConditionRegistry::default();
    let requested: Vec<String> = inputs
        .policy
        .approvals
        .hard_stops
        .iter()
        .map(|h| h.name.clone())
        .collect();
    let mut hits = registry.evaluate(&requested, inputs.pack, &bound_owned);
    hits.extend(inputs.external_hard_stops.iter().cloned());

    let receipt_refs: Vec<VerdictReceiptRef> = bound
        .iter()
        .map(|r| VerdictReceiptRef {
            role: r.role,
            agent_id: r.agent_id.clone(),
            receipt_digest: r
                .raw_response_sha
                .clone()
                .unwrap_or_else(|| "sha256:0".into()),
            decision: r.decision,
            not_author: r.not_author,
        })
        .collect();

    let ttl_minutes = inputs.policy.approvals.verdict_ttl_minutes.unwrap_or(60) as i64;
    let now = Utc::now();
    let expires_at = now + Duration::minutes(ttl_minutes);

    if !hits.is_empty() {
        let verdict = VibeGateVerdict {
            schema: SchemaTag::new(),
            id: mint_verdict_id(now, &inputs.pack.head_sha),
            evidence_pack_id: inputs.pack.id.clone(),
            merge_request: inputs.merge_request.map(|s| s.to_string()),
            repo: inputs.repo.to_string(),
            target_branch: inputs.target_branch.to_string(),
            head_sha: inputs.pack.head_sha.clone(),
            policy_sha: inputs.pack.policy_sha.clone(),
            evidence_pack_digest: inputs.pack.evidence_digest.clone(),
            risk: inputs.pack.risk,
            hard_stops: hits.iter().map(|h| h.name.clone()).collect(),
            required_reviews: inputs
                .policy
                .quorum_for(inputs.pack.risk)
                .map_or_else(Vec::new, |q| q.roles.clone()),
            approval_receipts: receipt_refs,
            decision: GateDecision::Reject,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at,
            created_at: now,
            signature: Signature::stub(),
        };
        return JudgeOutcome {
            verdict,
            dropped_receipts: dropped,
        };
    }

    // 3. Quorum.
    let outcome = evaluate_quorum(
        inputs.pack.risk,
        &bound_owned,
        &inputs.policy.approvals,
        inputs.author_agent,
    );
    let (decision, _reason) = match outcome.decision {
        QuorumDecision::Met => (GateDecision::AllowMerge, outcome.reason),
        QuorumDecision::HumanRequired => (GateDecision::RequireHuman, outcome.reason),
        QuorumDecision::Insufficient => (GateDecision::RequireHuman, outcome.reason),
        QuorumDecision::Vetoed => (GateDecision::Reject, outcome.reason),
    };

    let verdict = VibeGateVerdict {
        schema: SchemaTag::new(),
        id: mint_verdict_id(now, &inputs.pack.head_sha),
        evidence_pack_id: inputs.pack.id.clone(),
        merge_request: inputs.merge_request.map(|s| s.to_string()),
        repo: inputs.repo.to_string(),
        target_branch: inputs.target_branch.to_string(),
        head_sha: inputs.pack.head_sha.clone(),
        policy_sha: inputs.pack.policy_sha.clone(),
        evidence_pack_digest: inputs.pack.evidence_digest.clone(),
        risk: inputs.pack.risk,
        hard_stops: vec![],
        required_reviews: inputs
            .policy
            .quorum_for(inputs.pack.risk)
            .map_or_else(Vec::new, |q| q.roles.clone()),
        approval_receipts: receipt_refs,
        decision,
        valid_for_head_sha_only: true,
        rebind_on_train: true,
        expires_at,
        created_at: now,
        signature: Signature::stub(),
    };

    JudgeOutcome {
        verdict,
        dropped_receipts: dropped,
    }
}

fn mint_verdict_id(now: chrono::DateTime<Utc>, head_sha: &str) -> String {
    let ts_hex = format!("{:013X}", now.timestamp_millis() as u64);
    let tail: String = head_sha
        .chars()
        .rev()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(13)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let mut s = format!("vgv_{ts_hex}{tail}");
    while s.len() < 30 {
        s.push('0');
    }
    s.truncate(30);
    s
}

// Convenience extension trait so we can write `policy.quorum_for(tier)`.
impl PolicyBundle {
    pub fn quorum_for(
        &self,
        tier: crate::autonomy::types::RiskTier,
    ) -> Option<&crate::autonomy::policy_yaml::QuorumEntry> {
        self.approvals.quorum.get(&tier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::evidence::{EvidenceInputs, build_evidence_pack};
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::*;
    use chrono::Utc;
    use std::path::Path;

    fn bundle() -> PolicyBundle {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".jeryu/autonomy/policies");
        PolicyBundle::from_dir(&dir).expect("loads")
    }

    fn pack_at_tier(tier: RiskTier, signed: bool, secret_failed: bool) -> EvidencePack {
        let mut p = build_evidence_pack(EvidenceInputs {
            repo: "org/p",
            source_branch: "agent/x",
            target_branch: "main",
            head_sha: "a".repeat(40).leak(),
            base_sha: "b".repeat(40).leak(),
            policy_sha: "c".repeat(40).leak(),
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
        });
        if signed {
            // Use ed25519 algo so cond_evidence_signature_invalid accepts it.
            // The judge does not verify the signature bytes against a key here
            // — it only checks the declared algo. A separate verification path
            // would consume the pubkey from .jeryu/autonomy/keys/.
            p.signature = Some(Signature {
                key_id: "evidence-builder.v1".into(),
                algo: "ed25519".into(),
                value: "0".repeat(128),
            });
        }
        p
    }

    fn receipt(
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
            signature: Signature {
                key_id: format!("{agent}.ed25519"),
                algo: "sha256-hmac-stub".into(),
                value: "0".repeat(64),
            },
        }
    }

    #[test]
    fn allow_merge_when_quorum_met_no_hard_stops() {
        let b = bundle();
        let p = pack_at_tier(RiskTier::R2, true, false);
        let receipts = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
            receipt(
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
            merge_request: Some("!1"),
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
        let p = pack_at_tier(RiskTier::R2, true, false);
        let receipts = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Block, &p),
            receipt(
                ReviewerRole::TestIntegrity,
                "test.v1",
                ReviewDecision::Pass,
                &p,
            ),
            receipt(ReviewerRole::Runtime, "rt.v1", ReviewDecision::Pass, &p),
        ];
        let out = judge(JudgeInputs {
            pack: &p,
            receipts: &receipts,
            policy: &b,
            repo: "org/p",
            target_branch: "main",
            merge_request: None,
            author_agent: None,
            external_hard_stops: &[],
        });
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
        let p = pack_at_tier(RiskTier::R2, true, true); // secret_scan_failed
        let receipts = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
            receipt(
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
            merge_request: None,
            author_agent: None,
            external_hard_stops: &[],
        });
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
        let p = pack_at_tier(RiskTier::R2, true, false);
        let mut bad = receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p);
        bad.head_sha = "d".repeat(40);
        let good = receipt(
            ReviewerRole::TestIntegrity,
            "test.v1",
            ReviewDecision::Pass,
            &p,
        );
        let receipts = vec![bad, good];
        let out = judge(JudgeInputs {
            pack: &p,
            receipts: &receipts,
            policy: &b,
            repo: "org/p",
            target_branch: "main",
            merge_request: None,
            author_agent: None,
            external_hard_stops: &[],
        });
        // Drift drops the security receipt → missing role → require_human.
        assert_eq!(out.dropped_receipts.len(), 1);
        assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
    }

    #[test]
    fn unsigned_pack_fails_closed_via_evidence_signature_invalid() {
        let b = bundle();
        let p = pack_at_tier(RiskTier::R2, false, false); // pack NOT signed
        let receipts = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
            receipt(
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
            merge_request: None,
            author_agent: None,
            external_hard_stops: &[],
        });
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
        use crate::autonomy::conditions::HardStop;
        let b = bundle();
        let p = pack_at_tier(RiskTier::R2, true, false);
        let receipts = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass, &p),
            receipt(
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
            merge_request: None,
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
        let p = pack_at_tier(RiskTier::R4, true, false);
        // R4 quorum has approvals_needed=0 and human_required=true → RequireHuman.
        let out = judge(JudgeInputs {
            pack: &p,
            receipts: &[],
            policy: &b,
            repo: "org/p",
            target_branch: "main",
            merge_request: None,
            author_agent: None,
            external_hard_stops: &[],
        });
        assert_eq!(out.verdict.decision, GateDecision::RequireHuman);
    }

    // --- Wave 5 coverage-boost addition ------------------------------------

    /// `mint_verdict_id` must produce a 30-character id prefixed with
    /// `vgv_`. Both fields are part of the wire format and changing either
    /// requires a coordinated schema bump.
    #[test]
    fn mint_verdict_id_is_30_chars_and_prefixed() {
        let now = chrono::Utc::now();
        let head = "f".repeat(40);
        let id = mint_verdict_id(now, &head);
        assert!(id.starts_with("vgv_"), "id must start with vgv_; got {id}");
        assert_eq!(
            id.len(),
            30,
            "id must be exactly 30 chars; got `{id}` (len {})",
            id.len()
        );
        // A short head still produces a valid 30-char id (pads with '0').
        let short_head = "abc";
        let id2 = mint_verdict_id(now, short_head);
        assert!(id2.starts_with("vgv_"));
        assert_eq!(id2.len(), 30);
    }
}
