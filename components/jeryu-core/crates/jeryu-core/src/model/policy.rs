//! Branch protection rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchProtectionRule {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub required_status_checks: Vec<String>,
    pub required_approving_review_count: u64,
    pub enforce_admins: bool,
    pub required_linear_history: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub require_signed_commits: bool,
    pub require_jankurai_proof: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SetBranchProtectionRequest {
    #[serde(default)]
    pub required_status_checks: Vec<String>,
    #[serde(default)]
    pub required_approving_review_count: u64,
    #[serde(default)]
    pub enforce_admins: bool,
    #[serde(default)]
    pub required_linear_history: bool,
    #[serde(default)]
    pub allow_force_pushes: bool,
    #[serde(default)]
    pub allow_deletions: bool,
    #[serde(default)]
    pub require_signed_commits: bool,
    #[serde(default)]
    pub require_jankurai_proof: bool,
}
