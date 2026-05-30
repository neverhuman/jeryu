//! Judge agent — pure policy fusion.
//!
//! Takes an `EvidencePack`, a set of signed receipts, and the `PolicyBundle`
//! loaded from the target branch (Law 3). Emits a `VibeGateVerdict`. **The
//! judge never reads code** — eliminating the LLM attack surface for the
//! fusion step.
//!
//! Order of operations:
//!   1. SHA-bind every receipt to the pack. Drift → drop, log.
//!   2. Walk the policy `hard_stops` through the conditions registry, then
//!      merge externally-injected hard stops. ANY hit → Reject (veto >
//!      approval count).
//!   3. Evaluate quorum for the pack's risk tier.
//!   4. Map quorum outcome to AllowMerge / RequireHuman / Reject.

use crate::approval::quorum::{QuorumDecision, evaluate_quorum};
use crate::approval::sha_bind::verify_sha_binding;
use crate::conditions::{ConditionRegistry, HardStop};
use crate::policy::PolicyBundle;
use crate::schema::{
    AgentApprovalReceipt, EvidencePack, GateDecision, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
};
use crate::signing::Signature;
use chrono::{Duration, Utc};

pub struct JudgeInputs<'a> {
    pub pack: &'a EvidencePack,
    pub receipts: &'a [AgentApprovalReceipt],
    pub policy: &'a PolicyBundle,
    pub repo: &'a str,
    pub target_branch: &'a str,
    /// Pull-request reference (PR model, not the legacy MR model).
    pub pull_request: Option<&'a str>,
    pub author_agent: Option<&'a str>,
    /// Hard stops the orchestrator pre-computed (e.g. `codeowners_not_satisfied`,
    /// `freeze_window_active`, `budget_exceeded`). Merged with registry-computed
    /// hits; ANY hit → Reject (veto > approval).
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
            pull_request: None,
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

    // 2. Hard stops: registry-computed + caller-injected.
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

    let required_reviews = inputs
        .policy
        .quorum_for(inputs.pack.risk)
        .map_or_else(Vec::new, |q| q.roles.clone());

    if !hits.is_empty() {
        let verdict = build_verdict(
            &inputs,
            now,
            expires_at,
            hits.iter().map(|h| h.name.clone()).collect(),
            required_reviews,
            receipt_refs,
            GateDecision::Reject,
        );
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
    let decision = match outcome.decision {
        QuorumDecision::Met => GateDecision::AllowMerge,
        QuorumDecision::HumanRequired | QuorumDecision::Insufficient => GateDecision::RequireHuman,
        QuorumDecision::Vetoed => GateDecision::Reject,
    };

    let verdict = build_verdict(
        &inputs,
        now,
        expires_at,
        vec![],
        required_reviews,
        receipt_refs,
        decision,
    );

    JudgeOutcome {
        verdict,
        dropped_receipts: dropped,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_verdict(
    inputs: &JudgeInputs<'_>,
    now: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
    hard_stops: Vec<String>,
    required_reviews: Vec<crate::schema::ReviewerRole>,
    approval_receipts: Vec<VerdictReceiptRef>,
    decision: GateDecision,
) -> VibeGateVerdict {
    VibeGateVerdict {
        schema: SchemaTag::new(),
        id: mint_verdict_id(now, &inputs.pack.head_sha),
        evidence_pack_id: inputs.pack.id.clone(),
        pull_request: inputs.pull_request.map(|s| s.to_string()),
        repo: inputs.repo.to_string(),
        target_branch: inputs.target_branch.to_string(),
        head_sha: inputs.pack.head_sha.clone(),
        policy_sha: inputs.pack.policy_sha.clone(),
        evidence_pack_digest: inputs.pack.evidence_digest.clone(),
        risk: inputs.pack.risk,
        hard_stops,
        required_reviews,
        approval_receipts,
        decision,
        valid_for_head_sha_only: true,
        rebind_on_train: true,
        expires_at,
        created_at: now,
        signature: Signature::stub(),
    }
}

/// Mint a 30-char verdict id prefixed `vgv_`. Both the length and prefix are
/// part of the wire format.
pub fn mint_verdict_id(now: chrono::DateTime<Utc>, head_sha: &str) -> String {
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

#[cfg(test)]
mod tests {
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
}
