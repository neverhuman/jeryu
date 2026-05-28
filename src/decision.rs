//! Owner: Agent Decision Engine (Risk Gates, Supersedence, Impact Classification)
//! Proof: `cargo test -p jeryu -- decision`
//! Invariants: All agent outcomes flow through evaluate_risk_gate; supersedence and impact are typed enums, never raw strings; RiskGateDecision must be checked before any merge or promotion

use crate::capsule::FailureCapsule;

#[path = "decision_gate.rs"]
mod decision_gate;
pub use decision_gate::*;

#[path = "decision_types.rs"]
mod decision_types;
pub use decision_types::*;

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

    if capsule.failure_kind == crate::ci_failure::SOURCE_FETCH_AUTH_FAILURE_KIND
        || crate::ci_failure::is_source_fetch_auth_failure(&capsule.log_snippet)
    {
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

pub fn failure_response_for(capsule: &FailureCapsule) -> RetryDecision {
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

pub fn recommend_recovery(capsule: &FailureCapsule) -> RetryDecision {
    failure_response_for(capsule)
}

pub fn is_branch_creation_push(before_sha: &str) -> bool {
    before_sha == "0000000000000000000000000000000000000000"
}
