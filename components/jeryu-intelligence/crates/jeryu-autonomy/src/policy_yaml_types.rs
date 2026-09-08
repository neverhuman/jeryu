//! Strict-typed policy model for `.jeryu/autonomy/policies/*.yml`.
//!
//! Decision: YAML-only policy with named-condition references; no DSL. All
//! structs use `#[serde(deny_unknown_fields)]` so policy drift fails closed.

use crate::types::{ReviewerRole, RiskTier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- risk.yml ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RiskMatcher {
    #[serde(default)]
    pub paths_match: Vec<String>,
    #[serde(default)]
    pub paths_only_in: Vec<String>,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub max_lines_changed: Option<u32>,
    #[serde(default)]
    pub lines_changed_gte: Option<u32>,
    #[serde(default)]
    pub lines_changed_lte: Option<u32>,
    #[serde(default)]
    pub all_files_have_targeted_tests: Option<bool>,
    #[serde(default)]
    pub any_path_matches_protected: Option<bool>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskTierEntry {
    pub id: RiskTier,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub matchers: Vec<RiskMatcher>,
    #[serde(default)]
    pub auto_merge: bool,
    #[serde(default)]
    pub human_required: bool,
    #[serde(default)]
    pub fail_closed: bool,
    #[serde(default)]
    pub required_reviews: Vec<ReviewerRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskPolicy {
    pub schema: String,
    pub tiers: Vec<RiskTierEntry>,
    #[serde(default)]
    pub evaluation_order: Option<String>,
}

// --- approvals.yml -------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct HardStopEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct ApprovalsPolicy {
    pub schema: String,
    /// Required invariants for approval evaluation.
    pub invariants: ApprovalRules,
    #[serde(default)]
    pub hard_stops: Vec<HardStopEntry>,
    /// CI / required-check lanes that MUST be green (conclusion `Success`) before
    /// any merge. Each lane name is matched against the pack's `ci_status`; a
    /// lane that is absent fires `missing_required_ci_check`, and a lane present
    /// with a non-`Success` conclusion fires `failed_required_ci_check`. Empty
    /// (the default) means no CI gate — back-compat for repos that haven't opted
    /// in.
    #[serde(default)]
    pub required_ci_lanes: Vec<String>,
    /// Per-tier quorum, keyed by `R0..R5`.
    pub quorum: HashMap<RiskTier, QuorumEntry>,
    #[serde(default)]
    pub verdict_ttl_minutes: Option<u32>,
    #[serde(default)]
    pub re_judge_on: Vec<String>,
}

// --- release.yml ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanaryRollbackOn {
    pub error_rate_relative_increase: f64,
    pub p95_latency_relative_increase: f64,
    pub crash_loop: bool,
    pub security_signal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanaryRules {
    #[serde(default)]
    pub initial_percent: u8,
    #[serde(default)]
    pub max_percent_without_human: u8,
    #[serde(default)]
    pub analysis_minutes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_on: Option<CanaryRollbackOn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightwatchRules {
    pub may_rollback: bool,
    pub may_promote: bool,
    pub may_pause_pipeline: bool,
    pub may_page_human: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseBuildRules {
    pub build_once: bool,
    pub require_sbom: bool,
    pub require_slsa_provenance: bool,
    pub require_artifact_signature: bool,
    pub require_rollback_plan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePolicy {
    pub schema: String,
    pub build: ReleaseBuildRules,
    #[serde(default)]
    pub canary: Option<CanaryRules>,
    #[serde(default)]
    pub nightwatch: Option<NightwatchRules>,
    #[serde(default)]
    pub release_ready_receipts: Vec<String>,
}

// --- protected-paths.yml -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedPathsPolicy {
    pub schema: String,
    /// Paths whose change ALWAYS requires a human (R4 floor).
    pub hard_human: Vec<String>,
    /// Path-based semantic triggers (logic lives in conditions registry).
    #[serde(default)]
    pub semantic_triggers: Vec<String>,
}

// --- freeze.yml ----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FreezeRules {
    #[serde(default)]
    pub weekends: bool,
    #[serde(default)]
    pub dates: Vec<String>,
    #[serde(default)]
    pub hours: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreezePolicy {
    pub schema: String,
    #[serde(default)]
    pub freeze: FreezeRules,
}

fn true_default() -> bool {
    true
}
