//! Error model for Phase 7.

use serde::Serialize;
use std::fmt::{Display, Formatter};

/// Convenient result alias for Jeryu operations.
pub type JeryuResult<T> = Result<T, JeryuError>;

/// Machine-readable repair guidance for humans and local agents.
///
/// Every field is deliberately concrete: a caller can render this next to the
/// error, route it into a proof receipt, or hand it to an agent without asking
/// the agent to infer intent from display text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AgentRepairHint {
    /// Stable purpose of the failed operation.
    pub purpose: &'static str,
    /// Stable reason category for routing and receipts.
    pub reason: &'static str,
    /// Common local fixes to try before broad investigation.
    pub common_fixes: &'static [&'static str],
    /// Local documentation URL or path for the owning contract.
    pub docs_url: &'static str,
    /// One-line rerun hint for the next local proof loop.
    pub repair_hint: &'static str,
}

/// Policy-aware errors. These are intentionally explicit so failures are
/// repairable by humans and agents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JeryuError {
    /// A requested entity was not found.
    NotFound(String),
    /// Input was malformed or semantically invalid.
    Invalid(String),
    /// A Jankurai or queue policy denied the operation.
    PolicyDenied(String),
    /// The merge queue detected a conflict.
    Conflict(String),
    /// A required receipt was missing.
    MissingReceipt(String),
    /// A proof witness was required but absent or invalid.
    MissingProofWitness(String),
}

impl Display for JeryuError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid: {msg}"),
            Self::PolicyDenied(msg) => write!(f, "policy denied: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::MissingReceipt(msg) => write!(f, "missing receipt: {msg}"),
            Self::MissingProofWitness(msg) => write!(f, "missing proof witness: {msg}"),
        }
    }
}

impl std::error::Error for JeryuError {}

impl JeryuError {
    /// Return the structured repair contract that corresponds to this error.
    pub fn agent_repair_hint(&self) -> AgentRepairHint {
        match self {
            Self::NotFound(_) => AgentRepairHint {
                purpose: "load requested domain entity",
                reason: "not_found",
                common_fixes: &[
                    "verify the repository, pull request, receipt, or queue id",
                    "rerun the read model refresh before retrying mutation",
                ],
                docs_url: "docs/errors.md#not-found",
                repair_hint: "rerun the narrow read-model or domain test for the owning path",
            },
            Self::Invalid(_) => AgentRepairHint {
                purpose: "validate request boundary",
                reason: "invalid_input",
                common_fixes: &[
                    "check typed ids and request fields before entering domain code",
                    "add a boundary test for the rejected input shape",
                ],
                docs_url: "docs/errors.md#invalid-input",
                repair_hint: "rerun the crate boundary tests and the mapped proof lane",
            },
            Self::PolicyDenied(_) => AgentRepairHint {
                purpose: "enforce repository or CI policy",
                reason: "policy_denied",
                common_fixes: &[
                    "inspect the policy reason list instead of bypassing the guard",
                    "add required proof, approval, or trust receipt",
                ],
                docs_url: "docs/errors.md#policy-denied",
                repair_hint: "rerun owner/test-map checks and the policy crate tests",
            },
            Self::Conflict(_) => AgentRepairHint {
                purpose: "preserve merge and state consistency",
                reason: "conflict",
                common_fixes: &[
                    "refresh the base state and recompute the merge witness",
                    "retry through the queue instead of mutating state directly",
                ],
                docs_url: "docs/errors.md#conflict",
                repair_hint: "rerun scheduler and merge-queue tests",
            },
            Self::MissingReceipt(_) => AgentRepairHint {
                purpose: "bind mutation to durable evidence",
                reason: "missing_receipt",
                common_fixes: &[
                    "produce the required signed receipt before retrying",
                    "verify the receipt id, digest, and operation kind",
                ],
                docs_url: "docs/errors.md#missing-receipt",
                repair_hint: "rerun the release, cache, or scheduler receipt test for the path",
            },
            Self::MissingProofWitness(_) => AgentRepairHint {
                purpose: "prove change eligibility before merge",
                reason: "missing_proof_witness",
                common_fixes: &[
                    "run the mapped proof lane for every changed owned path",
                    "regenerate the witness at the exact head commit",
                ],
                docs_url: "docs/errors.md#missing-proof-witness",
                repair_hint: "rerun proof-lane checks and merge-queue witness tests",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JeryuError;

    #[test]
    fn every_error_has_agent_repair_hint() {
        let errors = [
            JeryuError::NotFound("repo".to_string()),
            JeryuError::Invalid("id".to_string()),
            JeryuError::PolicyDenied("branch".to_string()),
            JeryuError::Conflict("merge".to_string()),
            JeryuError::MissingReceipt("release".to_string()),
            JeryuError::MissingProofWitness("owner".to_string()),
        ];

        for error in errors {
            let hint = error.agent_repair_hint();
            assert!(!hint.purpose.is_empty());
            assert!(!hint.reason.is_empty());
            assert!(!hint.common_fixes.is_empty());
            assert!(hint.docs_url.starts_with("docs/errors.md#"));
            assert!(hint.repair_hint.contains("rerun"));
        }
    }
}
