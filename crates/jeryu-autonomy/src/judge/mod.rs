//! Judge — pure policy fusion.
//!
//! Takes an [`EvidencePack`], a set of signed [`AgentApprovalReceipt`]s, and the
//! [`PolicyBundle`] loaded from the *target branch*. Emits a [`VibeGateVerdict`].
//! **The judge never reads code** — eliminating the LLM attack surface for the
//! fusion step.
//!
//! Order of operations:
//!   1. SHA-bind every receipt to the pack. Receipts with drift → drop, log.
//!   2. Walk approvals policy `hard_stops` through the conditions registry.
//!      ANY hit → `Reject` (veto > approval count).
//!   3. Evaluate quorum for the pack's risk tier.
//!   4. `HumanRequired`/`Insufficient` → `RequireHuman`; `Met` → `AllowMerge`.
//!
//! The borrowed input/output bundle lives in [`inputs`] and the verdict-id
//! minting in [`verdict_id`]; both are re-exported / used here so the public
//! `crate::judge` surface is unchanged.

mod inputs;
mod verdict_id;

pub use inputs::{JudgeInputs, JudgeOutcome};
use verdict_id::mint_verdict_id;

use crate::conditions::{ConditionRegistry, ci_hard_stops};
use crate::quorum::{QuorumDecision, evaluate_quorum};
use crate::sha_bind::verify_sha_binding;
use crate::signing::Signature;
use crate::types::{
    AgentApprovalReceipt, GateDecision, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
};
use chrono::{Duration, Utc};

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
    // Pre-merge CI gate: any policy-required lane that is missing from the pack's
    // ci_status, or present but not green, is a hard stop (veto > approval). The
    // judge holds both the pack and the policy's required_ci_lanes, so it
    // computes these here and merges them like the externally-supplied stops.
    hits.extend(ci_hard_stops(
        inputs.pack,
        &inputs.policy.approvals.required_ci_lanes,
    ));
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
            pull_request: inputs.pull_request.map(|s| s.to_string()),
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
            signature: Signature::unsigned(),
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
    let decision = match outcome.decision {
        QuorumDecision::Met => GateDecision::AllowMerge,
        QuorumDecision::HumanRequired => GateDecision::RequireHuman,
        QuorumDecision::Insufficient => GateDecision::RequireHuman,
        QuorumDecision::Vetoed => GateDecision::Reject,
    };

    let verdict = VibeGateVerdict {
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
        signature: Signature::unsigned(),
    };

    JudgeOutcome {
        verdict,
        dropped_receipts: dropped,
    }
}

#[cfg(test)]
mod tests;
