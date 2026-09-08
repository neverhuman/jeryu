//! Reviewer family: the shared dispatch flow ([`runner`]), the per-role
//! reviewers, and the [`Reviewer`] trait seam that lets the orchestrator drive
//! an LLM-backed reviewer or a deterministic reviewer interchangeably.

pub mod lockfile;
pub mod nightwatch;
pub mod runner;
pub mod runtime;
pub mod security;
pub mod test_integrity;

pub use runner::{ReviewInputs, ReviewerCallError, ReviewerRoleId, run_review, sign_receipt};

use crate::schema::{AgentApprovalReceipt, EvidencePack, ReviewerRole};
use async_trait::async_trait;

/// Context handed to a [`Reviewer`] for a single role on a single pack. The diff
/// is the change-under-review; the receipt's SHA-bind tuple comes from `pack`.
pub struct ReviewContext<'a> {
    pub role: ReviewerRole,
    pub pack: &'a EvidencePack,
    pub diff: &'a str,
    /// The role's system prompt markdown (used for prompt_sha + the LLM call).
    pub system_prompt_markdown: &'a str,
}

/// A reviewer produces one structured receipt per call. LLM calls live behind
/// this seam; a [`DeterministicReviewer`] stands in for tests.
///
/// The contract: on its own internal failure a reviewer SHOULD return an
/// `Abstain` receipt rather than erroring, so one reviewer never aborts the
/// batch. It returns `Err` only for unrecoverable input violations (e.g. a
/// secret leaked into the diff) where the orchestrator must fail closed.
#[async_trait]
pub trait Reviewer: Send + Sync {
    async fn review(
        &self,
        ctx: &ReviewContext<'_>,
    ) -> Result<AgentApprovalReceipt, ReviewerCallError>;
}

#[cfg(test)]
pub use deterministic::DeterministicReviewer;

#[cfg(test)]
mod deterministic {
    use super::*;
    use crate::schema::{ReviewDecision, SchemaTag, TokenCounts};
    use crate::signing::Signature;
    use chrono::Utc;

    /// Deterministic reviewer for tests: returns a fixed decision and binds the
    /// receipt to the pack's `(id, head_sha, policy_sha)` tuple.
    pub struct DeterministicReviewer {
        pub decision: ReviewDecision,
        pub reason: String,
    }

    impl DeterministicReviewer {
        pub fn passing() -> Self {
            Self {
                decision: ReviewDecision::Pass,
                reason: "deterministic pass".into(),
            }
        }
        pub fn blocking() -> Self {
            Self {
                decision: ReviewDecision::Block,
                reason: "deterministic block".into(),
            }
        }
    }

    #[async_trait]
    impl Reviewer for DeterministicReviewer {
        async fn review(
            &self,
            ctx: &ReviewContext<'_>,
        ) -> Result<AgentApprovalReceipt, ReviewerCallError> {
            let prompt_hash = crate::prompt_builder::prompt_sha(ctx.system_prompt_markdown);
            Ok(AgentApprovalReceipt {
                schema: SchemaTag::new(),
                id: format!("aar_deterministic_{:?}", ctx.role),
                evidence_pack_id: ctx.pack.id.clone(),
                role: ctx.role,
                agent_id: format!("reviewer-{:?}.deterministic", ctx.role).to_ascii_lowercase(),
                prompt_sha: Some(prompt_hash),
                provider: Some("deterministic".into()),
                model: Some("deterministic-model".into()),
                temperature: Some(0.0),
                seed: None,
                raw_response_sha: Some(format!("sha256:{}", "0".repeat(64))),
                head_sha: ctx.pack.head_sha.clone(),
                policy_sha: ctx.pack.policy_sha.clone(),
                decision: self.decision,
                reason: Some(self.reason.clone()),
                findings: vec![],
                not_author: true,
                tokens: TokenCounts::default(),
                created_at: Utc::now(),
                signature: Signature::unsigned(),
            })
        }
    }
}
