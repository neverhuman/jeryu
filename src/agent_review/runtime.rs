//! Runtime reviewer. Evaluates production behavior risk: performance,
//! memory, concurrency, migrations, observability, blast radius.
//!
//! Thin wrapper around `runner::run_review` with `ReviewerRoleId::Runtime`.

use crate::agent_review::runner::{ReviewInputs, ReviewerCallError, ReviewerRoleId, run_review};
use crate::autonomy::signing::EdSigningKey;
use crate::autonomy::types::AgentApprovalReceipt;
use crate::llm::LlmRouter;

pub struct RuntimeReviewInputs<'a> {
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

pub async fn run_runtime_review(
    router: &LlmRouter,
    inputs: &RuntimeReviewInputs<'_>,
) -> Result<AgentApprovalReceipt, ReviewerCallError> {
    run_review(
        router,
        &ReviewInputs {
            role: ReviewerRoleId::Runtime,
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
