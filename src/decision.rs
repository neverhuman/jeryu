//! Owner: Agent Decision Engine (Risk Gates, Supersedence, Impact Classification)
//! Proof: `cargo test -p jeryu -- decision`
//! Invariants: All agent outcomes flow through evaluate_risk_gate; supersedence and impact are typed enums, never raw strings; RiskGateDecision must be checked before any merge or promotion

use serde::{Deserialize, Serialize};

use crate::capsule::FailureCapsule;

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

pub fn classify_failure(capsule: &FailureCapsule) -> FailureClassification {
    let haystack = format!(
        "{} {} {}",
        capsule.failure_kind,
        capsule.summary,
        capsule.log_snippet.to_ascii_lowercase()
    );

    if capsule.failure_kind == "quarantined" {
        return FailureClassification::Infrastructure;
    }

    if haystack.contains("timed out")
        || haystack.contains("network")
        || haystack.contains("connection reset")
        || haystack.contains("transient failure")
        || haystack.contains("preparing environment")
        || haystack.contains("runner system failure")
    {
        return FailureClassification::Transient;
    }

    if haystack.contains("compile")
        || haystack.contains("clippy")
        || haystack.contains("assertion")
        || haystack.contains("test failed")
        || haystack.contains("mismatch")
    {
        return FailureClassification::Regression;
    }

    if capsule.exit_code == 124 || capsule.exit_code == 137 {
        return FailureClassification::Transient;
    }

    FailureClassification::Unknown
}

pub fn recommend_retry(capsule: &FailureCapsule) -> RetryDecision {
    match classify_failure(capsule) {
        FailureClassification::Infrastructure | FailureClassification::Transient => {
            if capsule.failure_kind == "quarantined" {
                RetryDecision::Quarantine
            } else {
                RetryDecision::RetryOnce
            }
        }
        FailureClassification::Regression => RetryDecision::DoNotRetry,
        FailureClassification::Unknown => RetryDecision::Escalate,
    }
}

pub fn is_branch_creation_push(before_sha: &str) -> bool {
    before_sha == "0000000000000000000000000000000000000000"
}

pub fn evaluate_risk_gate(
    trust_tier: TrustTier,
    successful_jobs: usize,
    pending_jobs: usize,
    failed_jobs: usize,
    policy: &RequiredEvidencePolicy,
) -> RiskEvaluation {
    if policy.require_no_recent_failures && failed_jobs > 0 {
        return RiskEvaluation {
            decision: RiskGateDecision::Deny,
            reason: "failed jobs are still present for the merge ref".to_string(),
            trust_tier,
        };
    }

    if policy.require_no_pending_jobs && pending_jobs > 0 {
        return RiskEvaluation {
            decision: RiskGateDecision::Escalate,
            reason: "pending or running jobs still exist for the merge ref".to_string(),
            trust_tier,
        };
    }

    if policy.require_successful_jobs && successful_jobs == 0 {
        return RiskEvaluation {
            decision: RiskGateDecision::Deny,
            reason: "no successful validation jobs were found for the merge ref".to_string(),
            trust_tier,
        };
    }

    match trust_tier {
        TrustTier::Untrusted => RiskEvaluation {
            decision: RiskGateDecision::Escalate,
            reason: "untrusted tier requires a human escalation before merge".to_string(),
            trust_tier,
        },
        TrustTier::Trusted | TrustTier::Privileged => RiskEvaluation {
            decision: RiskGateDecision::Allow,
            reason: "required evidence policy satisfied".to_string(),
            trust_tier,
        },
    }
}

/// Evaluates validation, selector, cache, and trust evidence into a merge proof.
pub fn evaluate_merge_gate(
    input: MergeGateInput,
    policy: &RequiredEvidencePolicy,
) -> MergeGateProof {
    let mut blockers = Vec::new();
    if input.selector_misses > 0 {
        blockers.push(format!(
            "test selector has {} unresolved miss(es)",
            input.selector_misses
        ));
    }
    if input.cache_taints > 0 {
        blockers.push(format!("{} active cache taint(s)", input.cache_taints));
    }
    if policy.require_vti_receipt {
        match &input.vti_receipt {
            Some(receipt) => {
                if !receipt.skipped_tests_explained && receipt.mode != "full" {
                    blockers.push("VTI receipt does not explain skipped validation".to_string());
                }
                if let (Some(receipt_sha), Some(head_sha)) = (&receipt.head_sha, &input.head_sha)
                    && receipt_sha != head_sha
                {
                    blockers.push(format!(
                        "VTI receipt head SHA {} does not match merge head {}",
                        receipt_sha, head_sha
                    ));
                }
            }
            None => blockers.push("missing VTI validation receipt".to_string()),
        }
    }

    let risk = evaluate_risk_gate(
        input.trust_tier.clone(),
        input.successful_jobs,
        input.pending_jobs,
        input.failed_jobs,
        policy,
    );
    if risk.decision != RiskGateDecision::Allow {
        blockers.push(risk.reason.clone());
    }

    let decision = if blockers.is_empty() {
        RiskGateDecision::Allow
    } else if risk.decision == RiskGateDecision::Escalate {
        RiskGateDecision::Escalate
    } else {
        RiskGateDecision::Deny
    };

    MergeGateProof {
        decision,
        project_id: input.project_id,
        mr_iid: input.mr_iid,
        source_branch: input.source_branch,
        target_branch: input.target_branch,
        head_sha: input.head_sha,
        blockers,
        successful_jobs: input.successful_jobs,
        pending_jobs: input.pending_jobs,
        failed_jobs: input.failed_jobs,
        selector_misses: input.selector_misses,
        cache_taints: input.cache_taints,
        vti_receipt: input.vti_receipt,
        trust_tier: input.trust_tier,
        policy_version: "merge-gate-v3.01".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capsule::FailureCapsule;
    use std::collections::HashMap;

    fn capsule(kind: &str, log: &str, exit_code: i32) -> FailureCapsule {
        FailureCapsule {
            job_id: 1,
            pipeline_id: Some(2),
            project_id: 3,
            stage: "test".into(),
            exit_code,
            commit_sha: "abc".into(),
            ref_name: "main".into(),
            working_directory: "/builds/project".into(),
            log_snippet: log.into(),
            repro_script: "cargo test".into(),
            environment: HashMap::new(),
            failure_kind: kind.into(),
            summary: "summary".into(),
            superseded_by_sha: None,
            retried_from_job_id: None,
        }
    }

    fn ok_vti_receipt() -> VtiReceiptSummary {
        VtiReceiptSummary {
            receipt_id: "vti-ok".into(),
            mode: "full".into(),
            head_sha: Some("abc".into()),
            skipped_tests_explained: true,
            widened_to_full: false,
        }
    }

    fn merge_gate_input_fixture() -> MergeGateInput {
        MergeGateInput {
            project_id: 1,
            mr_iid: 2,
            source_branch: "agent/task".into(),
            target_branch: "main".into(),
            head_sha: Some("abc".into()),
            successful_jobs: 3,
            pending_jobs: 0,
            failed_jobs: 0,
            selector_misses: 0,
            cache_taints: 0,
            vti_receipt: Some(ok_vti_receipt()),
            trust_tier: TrustTier::Trusted,
        }
    }

    #[test]
    fn classifies_transient_failures() {
        let result = classify_failure(&capsule("timeout", "network connection reset", 1));
        assert_eq!(result, FailureClassification::Transient);
    }

    #[test]
    fn recommends_retry_for_transient_failures() {
        let result = recommend_retry(&capsule("timeout", "timed out", 124));
        assert_eq!(result, RetryDecision::RetryOnce);
    }

    #[test]
    fn risk_gate_denies_failed_refs() {
        let result = evaluate_risk_gate(
            TrustTier::Trusted,
            1,
            0,
            1,
            &RequiredEvidencePolicy::default(),
        );
        assert_eq!(result.decision, RiskGateDecision::Deny);
    }

    #[test]
    fn merge_gate_allows_clean_trusted_ref() {
        let proof = evaluate_merge_gate(
            merge_gate_input_fixture(),
            &RequiredEvidencePolicy::default(),
        );
        assert_eq!(proof.decision, RiskGateDecision::Allow);
        assert!(proof.blockers.is_empty());
    }

    #[test]
    fn merge_gate_denies_selector_miss_and_taint() {
        let mut input = merge_gate_input_fixture();
        input.selector_misses = 1;
        input.cache_taints = 2;
        let proof = evaluate_merge_gate(input, &RequiredEvidencePolicy::default());
        assert_eq!(proof.decision, RiskGateDecision::Deny);
        assert_eq!(proof.blockers.len(), 2);
    }

    #[test]
    fn merge_gate_denies_missing_vti_receipt() {
        let mut input = merge_gate_input_fixture();
        input.vti_receipt = None;
        let proof = evaluate_merge_gate(input, &RequiredEvidencePolicy::default());
        assert_eq!(proof.decision, RiskGateDecision::Deny);
        assert!(
            proof
                .blockers
                .contains(&"missing VTI validation receipt".to_string())
        );
    }
}
