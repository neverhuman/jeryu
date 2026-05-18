//! Lockfile-scout reviewer. Evaluates dependency / lockfile changes for
//! supply-chain risk. Most work is the static-check stage (`cargo-deny`,
//! license scan, yanked-package check) which runs outside the LLM; this
//! reviewer is the tiebreaker for ambiguous transitive changes.
//!
//! Thin wrapper around `runner::run_review` with `ReviewerRoleId::Lockfile`.

use crate::agent_review::runner::{ReviewInputs, ReviewerCallError, ReviewerRoleId, run_review};
use crate::autonomy::signing::EdSigningKey;
use crate::autonomy::types::AgentApprovalReceipt;
use crate::llm::LlmRouter;

pub struct LockfileReviewInputs<'a> {
    pub repo: &'a str,
    pub head_sha: &'a str,
    pub policy_sha: &'a str,
    pub target_branch: &'a str,
    pub evidence_pack_id: &'a str,
    pub diff: &'a str,
    pub system_prompt_markdown: &'a str,
    pub evidence_pack_json: Option<&'a str>,
    pub signing_key: Option<&'a EdSigningKey>,
}

pub async fn run_lockfile_review(
    router: &LlmRouter,
    inputs: &LockfileReviewInputs<'_>,
) -> Result<AgentApprovalReceipt, ReviewerCallError> {
    run_review(
        router,
        &ReviewInputs {
            role: ReviewerRoleId::Lockfile,
            repo: inputs.repo,
            head_sha: inputs.head_sha,
            policy_sha: inputs.policy_sha,
            target_branch: inputs.target_branch,
            evidence_pack_id: inputs.evidence_pack_id,
            diff: inputs.diff,
            system_prompt_markdown: inputs.system_prompt_markdown,
            evidence_pack_json: inputs.evidence_pack_json,
            signing_key: inputs.signing_key,
        },
    )
    .await
}
