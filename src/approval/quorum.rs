//! Quorum evaluator.
//!
//! Given a set of `AgentApprovalReceipt` and a `QuorumEntry` from
//! `.jeryu/autonomy/policies/approvals.yml`, decide whether the quorum is met,
//! short of one approval, or impossible.
//!
//! Tip1 invariants enforced here:
//!   - `no_self_approval` — author cannot count
//!   - `require_distinct_agent_identities` — duplicates don't count
//!   - All required `roles:` must emit `Pass`
//!   - Any `Block` decision short-circuits to `Vetoed`

use crate::autonomy::policy_yaml::{ApprovalsPolicy, QuorumEntry};
use crate::autonomy::types::{AgentApprovalReceipt, ReviewDecision, ReviewerRole, RiskTier};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumDecision {
    Met,
    Insufficient,
    Vetoed,
    HumanRequired,
}

#[derive(Debug, Clone)]
pub struct QuorumOutcome {
    pub decision: QuorumDecision,
    pub passing_roles: Vec<ReviewerRole>,
    pub blocking_roles: Vec<ReviewerRole>,
    pub missing_roles: Vec<ReviewerRole>,
    pub abstaining_roles: Vec<ReviewerRole>,
    pub reason: String,
}

pub fn evaluate_quorum(
    risk: RiskTier,
    receipts: &[AgentApprovalReceipt],
    policy: &ApprovalsPolicy,
    author_agent: Option<&str>,
) -> QuorumOutcome {
    let Some(entry) = policy.quorum.get(&risk).cloned() else {
        return QuorumOutcome {
            decision: QuorumDecision::HumanRequired,
            passing_roles: vec![],
            blocking_roles: vec![],
            missing_roles: vec![],
            abstaining_roles: vec![],
            reason: format!("missing quorum policy for {risk:?}; failing closed"),
        };
    };

    // Filter out author receipts (no self-approval).
    let mut counted: Vec<&AgentApprovalReceipt> = receipts.iter().collect();
    if policy.invariants.no_self_approval {
        if let Some(author) = author_agent {
            counted.retain(|r| r.agent_id != author && r.not_author);
        } else {
            counted.retain(|r| r.not_author);
        }
    }
    // Deduplicate by agent_id when require_distinct_agent_identities is set.
    if policy.invariants.require_distinct_agent_identities {
        let mut seen: HashSet<&str> = HashSet::new();
        counted.retain(|r| seen.insert(r.agent_id.as_str()));
    }

    let blocking_roles: Vec<ReviewerRole> = counted
        .iter()
        .filter(|r| matches!(r.decision, ReviewDecision::Block))
        .map(|r| r.role)
        .collect();
    if !blocking_roles.is_empty() {
        return QuorumOutcome {
            decision: QuorumDecision::Vetoed,
            passing_roles: vec![],
            blocking_roles,
            missing_roles: vec![],
            abstaining_roles: vec![],
            reason: "one or more reviewers issued a hard block".into(),
        };
    }

    let abstaining_roles: Vec<ReviewerRole> = counted
        .iter()
        .filter(|r| matches!(r.decision, ReviewDecision::Abstain))
        .map(|r| r.role)
        .collect();
    let passing_roles: Vec<ReviewerRole> = counted
        .iter()
        .filter(|r| matches!(r.decision, ReviewDecision::Pass))
        .map(|r| r.role)
        .collect();

    let missing_roles: Vec<ReviewerRole> = entry
        .roles
        .iter()
        .copied()
        .filter(|req| !passing_roles.contains(req))
        .collect();

    if !missing_roles.is_empty() || passing_roles.len() < entry.approvals_needed as usize {
        return QuorumOutcome {
            decision: QuorumDecision::Insufficient,
            passing_roles,
            blocking_roles,
            missing_roles,
            abstaining_roles,
            reason: format!(
                "needed {} approvals (roles {:?}); got {}",
                entry.approvals_needed,
                entry.roles,
                counted
                    .iter()
                    .filter(|r| matches!(r.decision, ReviewDecision::Pass))
                    .count()
            ),
        };
    }

    if entry.human_required {
        return QuorumOutcome {
            decision: QuorumDecision::HumanRequired,
            passing_roles,
            blocking_roles,
            missing_roles,
            abstaining_roles,
            reason: "policy requires human approval at this tier".into(),
        };
    }

    QuorumOutcome {
        decision: QuorumDecision::Met,
        passing_roles,
        blocking_roles,
        missing_roles,
        abstaining_roles,
        reason: "all required reviewers approved; quorum reached".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::*;
    use chrono::Utc;

    fn receipt(role: ReviewerRole, agent: &str, decision: ReviewDecision) -> AgentApprovalReceipt {
        AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: format!("aar_{agent}"),
            evidence_pack_id: "evp_x".into(),
            role,
            agent_id: agent.into(),
            prompt_sha: None,
            provider: None,
            model: None,
            temperature: None,
            seed: None,
            raw_response_sha: None,
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            decision,
            reason: None,
            findings: vec![],
            not_author: true,
            tokens: TokenCounts::default(),
            created_at: Utc::now(),
            signature: Signature::stub(),
        }
    }

    fn policy_with_quorum(
        tier: RiskTier,
        needed: u32,
        roles: Vec<ReviewerRole>,
        human: bool,
    ) -> ApprovalsPolicy {
        let mut p = ApprovalsPolicy {
            schema: "vibegate.approvals.v1".into(),
            invariants: ApprovalRulesDefault::default().0,
            hard_stops: vec![],
            quorum: std::collections::HashMap::new(),
            verdict_ttl_minutes: Some(60),
            re_judge_on: vec![],
        };
        p.quorum.insert(
            tier,
            QuorumEntry {
                approvals_needed: needed,
                roles,
                human_required: human,
                fail_closed: false,
                fail_closed_without_human: false,
            },
        );
        p
    }

    #[derive(Default)]
    struct ApprovalRulesDefault(crate::autonomy::policy_yaml::ApprovalRules);

    #[test]
    fn quorum_met_when_required_roles_pass() {
        let p = policy_with_quorum(
            RiskTier::R2,
            2,
            vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
            false,
        );
        let r = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass),
            receipt(ReviewerRole::TestIntegrity, "test.v1", ReviewDecision::Pass),
        ];
        let outcome = evaluate_quorum(RiskTier::R2, &r, &p, Some("builder.x"));
        assert_eq!(outcome.decision, QuorumDecision::Met);
    }

    #[test]
    fn one_block_vetoes_regardless_of_count() {
        let p = policy_with_quorum(
            RiskTier::R2,
            2,
            vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
            false,
        );
        let r = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Block),
            receipt(ReviewerRole::TestIntegrity, "test.v1", ReviewDecision::Pass),
            receipt(ReviewerRole::Runtime, "rt.v1", ReviewDecision::Pass),
        ];
        let outcome = evaluate_quorum(RiskTier::R2, &r, &p, None);
        assert_eq!(outcome.decision, QuorumDecision::Vetoed);
        assert_eq!(outcome.blocking_roles, vec![ReviewerRole::Security]);
    }

    #[test]
    fn missing_required_role_is_insufficient() {
        let p = policy_with_quorum(
            RiskTier::R2,
            2,
            vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
            false,
        );
        let r = vec![receipt(
            ReviewerRole::Security,
            "sec.v1",
            ReviewDecision::Pass,
        )];
        let outcome = evaluate_quorum(RiskTier::R2, &r, &p, None);
        assert_eq!(outcome.decision, QuorumDecision::Insufficient);
        assert!(outcome.missing_roles.contains(&ReviewerRole::TestIntegrity));
    }

    #[test]
    fn author_self_approval_does_not_count() {
        let p = policy_with_quorum(RiskTier::R1, 1, vec![ReviewerRole::TestIntegrity], false);
        // The "author" tries to self-approve as a reviewer.
        let mut self_r = receipt(
            ReviewerRole::TestIntegrity,
            "builder.author",
            ReviewDecision::Pass,
        );
        self_r.not_author = false;
        let outcome = evaluate_quorum(RiskTier::R1, &[self_r], &p, Some("builder.author"));
        assert_eq!(outcome.decision, QuorumDecision::Insufficient);
    }

    #[test]
    fn human_required_lands_separately() {
        let p = policy_with_quorum(RiskTier::R4, 0, vec![], true);
        let outcome = evaluate_quorum(RiskTier::R4, &[], &p, None);
        assert_eq!(outcome.decision, QuorumDecision::HumanRequired);
    }

    #[test]
    fn missing_quorum_policy_fails_closed() {
        let p = ApprovalsPolicy {
            schema: "vibegate.approvals.v1".into(),
            invariants: ApprovalRulesDefault::default().0,
            hard_stops: vec![],
            quorum: std::collections::HashMap::new(),
            verdict_ttl_minutes: Some(60),
            re_judge_on: vec![],
        };
        let outcome = evaluate_quorum(RiskTier::R1, &[], &p, None);
        assert_eq!(outcome.decision, QuorumDecision::HumanRequired);
        assert!(outcome.reason.contains("missing quorum policy"));
    }

    // --- Wave 5 coverage-boost addition ------------------------------------

    /// Author who submits a receipt with `not_author = true` (an honest
    /// misconfiguration on the agent side) must STILL be filtered out
    /// when `policy.invariants.no_self_approval` is set. We don't rely on
    /// the receipt's self-report alone.
    #[test]
    fn author_agent_cannot_self_approve_even_when_not_author_flag_true() {
        let p = policy_with_quorum(RiskTier::R1, 1, vec![ReviewerRole::TestIntegrity], false);
        let mut r = receipt(
            ReviewerRole::TestIntegrity,
            "builder.author",
            ReviewDecision::Pass,
        );
        // The agent sets not_author=true (incorrectly).
        r.not_author = true;
        // But the policy KNOWS the author is "builder.author".
        let outcome = evaluate_quorum(RiskTier::R1, &[r], &p, Some("builder.author"));
        assert_eq!(
            outcome.decision,
            QuorumDecision::Insufficient,
            "author_agent identity overrides a (lying) not_author flag"
        );
    }

    #[test]
    fn duplicate_agent_identities_collapse() {
        let p = policy_with_quorum(
            RiskTier::R2,
            2,
            vec![ReviewerRole::Security, ReviewerRole::TestIntegrity],
            false,
        );
        let r = vec![
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass),
            receipt(ReviewerRole::Security, "sec.v1", ReviewDecision::Pass),
        ];
        let outcome = evaluate_quorum(RiskTier::R2, &r, &p, None);
        assert_eq!(outcome.decision, QuorumDecision::Insufficient);
    }
}
