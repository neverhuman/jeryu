//! Approvals dashboard contract — pending pull requests awaiting human review.
//!
//! Provider-neutral, GitHub-shaped: pull requests are identified by a `number`
//! (no legacy MR `iid`), carry a CI `checks` status, a risk tier, and an age.
//! Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::freshness::SourceFreshness;
use crate::risk::RiskTier;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ApprovalsSnapshot {
    pub items: Vec<ApprovalItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<ApprovalsSummary>,
}

impl ApprovalsSnapshot {
    /// Count of pending PRs whose checks are red — drives the queue alert.
    pub fn failing_checks(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.checks.is_failing())
            .count() as u32
    }
}

/// One pull request awaiting a human approval decision (GitHub PR shape).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalItem {
    /// GitHub-style PR number (never a legacy merge-request iid).
    pub pr_number: u64,
    pub title: String,
    /// The agent (or author) that opened the PR.
    pub author: String,
    pub risk: RiskTier,
    /// Roll-up status of the PR's required CI checks.
    pub checks: CheckStatus,
    /// Human-friendly age, e.g. `3m`, `2h`, `4d`.
    pub age: String,
    /// Head commit SHA the checks ran against.
    pub head_sha: String,
}

impl ApprovalItem {
    pub fn new(pr_number: u64, title: impl Into<String>, risk: RiskTier) -> Self {
        Self {
            pr_number,
            title: title.into(),
            author: String::new(),
            risk,
            checks: CheckStatus::Pending,
            age: "0m".into(),
            head_sha: String::new(),
        }
    }
}

/// GitHub-style roll-up of a PR's required check runs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// All required checks succeeded.
    Success,
    /// One or more checks are still running / queued.
    Pending,
    /// At least one required check failed.
    Failure,
    /// No checks have reported yet.
    Neutral,
}

impl CheckStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Pending => "pending",
            Self::Failure => "failure",
            Self::Neutral => "neutral",
        }
    }

    pub fn is_failing(self) -> bool {
        matches!(self, Self::Failure)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ApprovalsSummary {
    pub pending_total: u32,
    pub checks_passing: u32,
    pub checks_failing: u32,
    pub high_risk_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = ApprovalsSnapshot::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
        assert_eq!(d.failing_checks(), 0);
    }

    #[test]
    fn check_status_round_trips() {
        let json = serde_json::to_string(&CheckStatus::Failure).unwrap();
        assert_eq!(json, "\"failure\"");
        let back: CheckStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CheckStatus::Failure);
        assert!(back.is_failing());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = ApprovalsSnapshot {
            items: vec![ApprovalItem::new(101, "fix flaky test", RiskTier::R2)],
            freshness: None,
            summary: Some(ApprovalsSummary::default()),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: ApprovalsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
        // GitHub PR number is on the wire; no legacy iid.
        assert!(json.contains("pr_number"));
        assert!(!json.contains("iid"));
    }
}
