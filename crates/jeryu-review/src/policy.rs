//! Approvals policy (loaded from the target branch per Law 3).
//!
//! In the fused workspace this is parsed from `approvals.yml` via `jeryu-gitd`.
//! Here we keep the typed shape plus a [`PolicyBundle::default_enforcing`]
//! constructor that mirrors the shipped default policy, so the judge/quorum can
//! run without a YAML dependency in isolation.

use crate::schema::{ReviewerRole, RiskTier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRules {
    #[serde(default = "true_default")]
    pub no_self_approval: bool,
    #[serde(default = "true_default")]
    pub exact_sha_required: bool,
    #[serde(default = "true_default")]
    pub target_branch_policy_only: bool,
    #[serde(default = "true_default")]
    pub fail_closed_on_missing_evidence: bool,
    #[serde(default = "true_default")]
    pub fail_closed_on_agent_disagreement: bool,
    #[serde(default = "true_default")]
    pub require_distinct_agent_identities: bool,
}

impl Default for ApprovalRules {
    fn default() -> Self {
        Self {
            no_self_approval: true,
            exact_sha_required: true,
            target_branch_policy_only: true,
            fail_closed_on_missing_evidence: true,
            fail_closed_on_agent_disagreement: true,
            require_distinct_agent_identities: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardStopEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuorumEntry {
    #[serde(default)]
    pub approvals_needed: u32,
    #[serde(default)]
    pub roles: Vec<ReviewerRole>,
    #[serde(default)]
    pub human_required: bool,
    #[serde(default)]
    pub fail_closed: bool,
    #[serde(default)]
    pub fail_closed_without_human: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalsPolicy {
    pub schema: String,
    pub invariants: ApprovalRules,
    #[serde(default)]
    pub hard_stops: Vec<HardStopEntry>,
    /// Per-tier quorum, keyed by `R0..R5`.
    pub quorum: HashMap<RiskTier, QuorumEntry>,
    #[serde(default)]
    pub verdict_ttl_minutes: Option<u32>,
    #[serde(default)]
    pub re_judge_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyBundle {
    pub approvals: ApprovalsPolicy,
}

impl PolicyBundle {
    /// The default enforcing policy used by the gate. Mirrors the shipped
    /// `approvals.yml`: R0–R2 auto-merge with a security + test-integrity
    /// quorum, R3+ require a human; the global hard-stop set includes the
    /// deterministic conditions the registry computes.
    pub fn default_enforcing() -> Self {
        let mut quorum = HashMap::new();
        quorum.insert(
            RiskTier::R0,
            QuorumEntry {
                approvals_needed: 1,
                roles: vec![ReviewerRole::TestIntegrity],
                human_required: false,
                fail_closed: false,
                fail_closed_without_human: false,
            },
        );
        quorum.insert(
            RiskTier::R1,
            QuorumEntry {
                approvals_needed: 1,
                roles: vec![ReviewerRole::TestIntegrity],
                human_required: false,
                fail_closed: false,
                fail_closed_without_human: false,
            },
        );
        quorum.insert(
            RiskTier::R2,
            QuorumEntry {
                approvals_needed: 2,
                roles: vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
                human_required: false,
                fail_closed: false,
                fail_closed_without_human: false,
            },
        );
        quorum.insert(
            RiskTier::R3,
            QuorumEntry {
                approvals_needed: 2,
                roles: vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
                human_required: true,
                fail_closed: false,
                fail_closed_without_human: true,
            },
        );
        // R4/R5: human-gated floor regardless of approvals.
        quorum.insert(
            RiskTier::R4,
            QuorumEntry {
                approvals_needed: 0,
                roles: vec![],
                human_required: true,
                fail_closed: true,
                fail_closed_without_human: true,
            },
        );
        quorum.insert(
            RiskTier::R5,
            QuorumEntry {
                approvals_needed: 0,
                roles: vec![],
                human_required: true,
                fail_closed: true,
                fail_closed_without_human: true,
            },
        );

        let hard_stops = [
            "evidence_signature_invalid",
            "secret_scan_failed",
            "secret_scan_missing",
            "sast_failed",
            "dependency_scan_failed",
            "reviewer_blocked",
        ]
        .into_iter()
        .map(|name| HardStopEntry {
            name: name.into(),
            reason: None,
        })
        .collect();

        Self {
            approvals: ApprovalsPolicy {
                schema: "vibegate.approvals.v1".into(),
                invariants: ApprovalRules::default(),
                hard_stops,
                quorum,
                verdict_ttl_minutes: Some(60),
                re_judge_on: vec![
                    "new_commit_on_pr".into(),
                    "policy_change_on_target".into(),
                    "verdict_ttl_expired".into(),
                ],
            },
        }
    }

    pub fn quorum_for(&self, tier: RiskTier) -> Option<&QuorumEntry> {
        self.approvals.quorum.get(&tier)
    }
}

fn true_default() -> bool {
    true
}
