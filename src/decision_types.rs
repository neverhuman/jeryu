use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupersedenceAction {
    Cancel,
    Preserve,
    Degrade,
    Ignore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersedenceDecision {
    pub project_id: i64,
    pub ref_name: String,
    pub newest_sha: String,
    pub superseded_pipeline_id: i64,
    pub superseded_sha: String,
    pub action: SupersedenceAction,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImpactLane {
    Full,
    Unit,
    Integration,
    DocsOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactDecision {
    pub project_id: i64,
    pub before: String,
    pub after: String,
    pub affected_paths: Vec<String>,
    pub selected_lanes: Vec<ImpactLane>,
    pub reason_codes: Vec<String>,
    pub widened_to_full: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureClassification {
    Infrastructure,
    Transient,
    Regression,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetryDecision {
    RetryOnce,
    DoNotRetry,
    Quarantine,
    Escalate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    Untrusted,
    Trusted,
    Privileged,
}

impl std::str::FromStr for TrustTier {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "untrusted" => Ok(Self::Untrusted),
            "privileged" => Ok(Self::Privileged),
            "trusted" => Ok(Self::Trusted),
            _ => Err(format!("unknown trust tier: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskGateDecision {
    Allow,
    Deny,
    Escalate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredEvidencePolicy {
    pub require_successful_jobs: bool,
    pub require_no_pending_jobs: bool,
    pub require_no_recent_failures: bool,
    pub require_vti_receipt: bool,
}

impl Default for RequiredEvidencePolicy {
    fn default() -> Self {
        Self {
            require_successful_jobs: true,
            require_no_pending_jobs: true,
            require_no_recent_failures: true,
            require_vti_receipt: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskEvaluation {
    pub decision: RiskGateDecision,
    pub reason: String,
    pub trust_tier: TrustTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Inputs required to produce a reusable merge-gate proof record.
pub struct MergeGateInput {
    /// GitLab project id.
    pub project_id: i64,
    /// GitLab merge request IID.
    pub mr_iid: i64,
    /// Source branch under review.
    pub source_branch: String,
    /// Target branch for merge.
    pub target_branch: String,
    /// Optional head SHA the evidence is bound to.
    pub head_sha: Option<String>,
    /// Successful validation job count for the merge ref.
    pub successful_jobs: usize,
    /// Pending or running validation job count for the merge ref.
    pub pending_jobs: usize,
    /// Failed validation job count for the merge ref.
    pub failed_jobs: usize,
    /// Unresolved selector misses relevant to this request.
    pub selector_misses: usize,
    /// Active cache taints relevant to this request.
    pub cache_taints: usize,
    /// VTI proof receipt for selected or skipped validation, when available.
    pub vti_receipt: Option<VtiReceiptSummary>,
    /// Trust tier of the actor or source branch.
    pub trust_tier: TrustTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Minimal VTI proof receipt material consumed by merge gates.
pub struct VtiReceiptSummary {
    /// Stable receipt id from the VTI planner or external testmap planner.
    pub receipt_id: String,
    /// Validation mode, such as full, selected, or docs_only.
    pub mode: String,
    /// Head SHA this receipt validates.
    pub head_sha: Option<String>,
    /// Whether skipped tests are explained by the receipt.
    pub skipped_tests_explained: bool,
    /// Whether the planner widened to full validation because evidence was incomplete.
    pub widened_to_full: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Versioned merge-gate proof record.
pub struct MergeGateProof {
    /// Final gate decision.
    pub decision: RiskGateDecision,
    /// GitLab project id.
    pub project_id: i64,
    /// GitLab merge request IID.
    pub mr_iid: i64,
    /// Source branch under review.
    pub source_branch: String,
    /// Target branch for merge.
    pub target_branch: String,
    /// Optional head SHA the evidence is bound to.
    pub head_sha: Option<String>,
    /// Blocking reasons that prevented allow.
    pub blockers: Vec<String>,
    /// Successful validation job count.
    pub successful_jobs: usize,
    /// Pending or running validation job count.
    pub pending_jobs: usize,
    /// Failed validation job count.
    pub failed_jobs: usize,
    /// Unresolved selector miss count.
    pub selector_misses: usize,
    /// Active cache taint count.
    pub cache_taints: usize,
    /// VTI receipt consumed by this proof, when available.
    pub vti_receipt: Option<VtiReceiptSummary>,
    /// Trust tier used by the decision.
    pub trust_tier: TrustTier,
    /// Version of the merge-gate policy contract.
    pub policy_version: String,
}
