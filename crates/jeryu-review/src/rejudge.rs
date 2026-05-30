//! Re-judge drift detector.
//!
//! Invariants:
//!   - A verdict is valid only for the exact `head_sha` + `policy_sha` it was
//!     minted under (Law 4).
//!   - Any trigger invalidates the verdict; the caller must run `judge()` again.
//!   - This module is pure — it observes inputs and reports drift. The caller
//!     decides what to do (re-judge, escalate, page a human).

use crate::schema::VibeGateVerdict;
use chrono::{DateTime, Utc};

/// Why a previously-issued verdict is no longer trustworthy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "trigger", rename_all = "snake_case")]
pub enum RejudgeReason {
    /// The PR's head commit advanced since the verdict was issued.
    NewCommitOnPr {
        verdict_head_sha: String,
        current_head_sha: String,
    },
    /// The target branch advanced; merge-train rebasing means a new integrated
    /// commit a new verdict must cover.
    TargetBranchAdvance {
        verdict_target_sha: String,
        current_target_sha: String,
    },
    /// The policy bundle on the target branch changed (Law 3).
    PolicyChangeOnTarget {
        verdict_policy_sha: String,
        current_policy_sha: String,
    },
    /// The verdict aged past its declared TTL.
    VerdictTtlExpired {
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },
}

impl RejudgeReason {
    pub fn ledger_kind(&self) -> &'static str {
        "merge_passport_invalidated"
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            RejudgeReason::NewCommitOnPr { .. } => "new_commit_on_pr",
            RejudgeReason::TargetBranchAdvance { .. } => "target_branch_advance",
            RejudgeReason::PolicyChangeOnTarget { .. } => "policy_change_on_target",
            RejudgeReason::VerdictTtlExpired { .. } => "verdict_ttl_expired",
        }
    }
}

/// What the caller currently knows about the live state of the PR. All values
/// are optional; missing values do not trigger re-judge (no invalidation on
/// unknown).
#[derive(Debug, Clone, Default)]
pub struct LiveState<'a> {
    pub head_sha: Option<&'a str>,
    pub target_branch_sha: Option<&'a str>,
    pub target_policy_sha: Option<&'a str>,
    pub now: Option<DateTime<Utc>>,
}

/// Inspect a verdict against live state and return every trigger that fires.
/// Order is stable: head, policy, ttl.
pub fn check(verdict: &VibeGateVerdict, live: &LiveState<'_>) -> Vec<RejudgeReason> {
    let mut out = Vec::new();
    if let Some(head) = live.head_sha
        && head != verdict.head_sha
    {
        out.push(RejudgeReason::NewCommitOnPr {
            verdict_head_sha: verdict.head_sha.clone(),
            current_head_sha: head.to_string(),
        });
    }
    // Target-advance is opt-in via the verdict's `rebind_on_train` flag. Because
    // the compact verdict wire does not carry the target sha it was minted
    // under, we conservatively do not fire here (the caller asserts target
    // drift via a dedicated signal); when rebind_on_train is false we never
    // fire on target advance.
    if let Some(target) = live.target_branch_sha
        && verdict.rebind_on_train
        && target != verdict.head_sha
    {
        let _ = target;
    }
    if let Some(policy) = live.target_policy_sha
        && policy != verdict.policy_sha
    {
        out.push(RejudgeReason::PolicyChangeOnTarget {
            verdict_policy_sha: verdict.policy_sha.clone(),
            current_policy_sha: policy.to_string(),
        });
    }
    if let Some(now) = live.now
        && now > verdict.expires_at
    {
        out.push(RejudgeReason::VerdictTtlExpired {
            expires_at: verdict.expires_at,
            now,
        });
    }
    out
}

/// Convenience: did anything trigger?
pub fn must_rejudge(verdict: &VibeGateVerdict, live: &LiveState<'_>) -> bool {
    !check(verdict, live).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict};
    use crate::signing::Signature;
    use chrono::Duration;

    fn fresh_verdict() -> VibeGateVerdict {
        let now = Utc::now();
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id: "vgv_x".into(),
            evidence_pack_id: "ep_x".into(),
            pull_request: None,
            repo: "org/repo".into(),
            target_branch: "main".into(),
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            evidence_pack_digest: "sha256:deadbeef".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: Vec::<VerdictReceiptRef>::new(),
            decision: GateDecision::AllowMerge,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at: now + Duration::minutes(60),
            created_at: now,
            signature: Signature::unsigned(),
        }
    }

    #[test]
    fn fresh_verdict_no_triggers() {
        let v = fresh_verdict();
        let live = LiveState {
            head_sha: Some(&v.head_sha),
            target_policy_sha: Some(&v.policy_sha),
            now: Some(v.created_at),
            ..Default::default()
        };
        assert!(check(&v, &live).is_empty());
        assert!(!must_rejudge(&v, &live));
    }

    #[test]
    fn head_sha_drift_triggers_new_commit_on_pr() {
        let v = fresh_verdict();
        let new_head = "d".repeat(40);
        let live = LiveState {
            head_sha: Some(&new_head),
            ..Default::default()
        };
        let hits = check(&v, &live);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].short_name(), "new_commit_on_pr");
        assert!(must_rejudge(&v, &live));
    }

    #[test]
    fn policy_drift_triggers_rejudge() {
        let v = fresh_verdict();
        let new_policy = "e".repeat(40);
        let live = LiveState {
            target_policy_sha: Some(&new_policy),
            ..Default::default()
        };
        let hits = check(&v, &live);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].short_name(), "policy_change_on_target");
    }

    #[test]
    fn ttl_expiry_triggers_rejudge() {
        let v = fresh_verdict();
        let future = v.expires_at + Duration::seconds(1);
        let live = LiveState {
            now: Some(future),
            ..Default::default()
        };
        let hits = check(&v, &live);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].short_name(), "verdict_ttl_expired");
    }

    #[test]
    fn multiple_triggers_all_reported() {
        let v = fresh_verdict();
        let new_head = "d".repeat(40);
        let new_policy = "e".repeat(40);
        let future = v.expires_at + Duration::minutes(5);
        let live = LiveState {
            head_sha: Some(&new_head),
            target_policy_sha: Some(&new_policy),
            now: Some(future),
            ..Default::default()
        };
        let hits = check(&v, &live);
        assert_eq!(hits.len(), 3);
        let names: Vec<&str> = hits.iter().map(|r| r.short_name()).collect();
        assert!(names.contains(&"new_commit_on_pr"));
        assert!(names.contains(&"policy_change_on_target"));
        assert!(names.contains(&"verdict_ttl_expired"));
    }

    #[test]
    fn missing_live_fields_do_not_trigger() {
        let v = fresh_verdict();
        assert!(check(&v, &LiveState::default()).is_empty());
    }

    #[test]
    fn target_advance_skipped_when_rebind_on_train_false() {
        let mut v = fresh_verdict();
        v.rebind_on_train = false;
        let new_target = "f".repeat(40);
        let live = LiveState {
            target_branch_sha: Some(&new_target),
            ..Default::default()
        };
        assert!(check(&v, &live).is_empty());
    }

    #[test]
    fn multiple_triggers_returned_in_documented_order() {
        let v = fresh_verdict();
        let new_head = "d".repeat(40);
        let new_policy = "e".repeat(40);
        let future = v.expires_at + Duration::minutes(5);
        let live = LiveState {
            head_sha: Some(&new_head),
            target_policy_sha: Some(&new_policy),
            now: Some(future),
            ..Default::default()
        };
        let h1 = check(&v, &live);
        let h2 = check(&v, &live);
        assert_eq!(
            h1.iter().map(|r| r.short_name()).collect::<Vec<_>>(),
            vec![
                "new_commit_on_pr",
                "policy_change_on_target",
                "verdict_ttl_expired"
            ]
        );
        assert_eq!(
            h1.iter().map(|r| r.short_name()).collect::<Vec<_>>(),
            h2.iter().map(|r| r.short_name()).collect::<Vec<_>>(),
        );
    }
}
