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
    /// Pull-request reference.
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
        signature: Signature::unsigned(),
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
mod tests;
