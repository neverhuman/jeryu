//! Test-integrity reviewer. Catches tests being silently weakened, deleted, or
//! turned into no-ops. Thin wrapper over the shared [`run_review`] flow.

use crate::llm::LlmRouter;
use crate::reviewers::runner::{ReviewInputs, ReviewerCallError, ReviewerRoleId, run_review};
use crate::schema::AgentApprovalReceipt;
use crate::signing::EdSigningKey;

pub struct TestIntegrityReviewInputs<'a> {
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

pub async fn run_test_integrity_review(
    router: &LlmRouter,
    inputs: &TestIntegrityReviewInputs<'_>,
) -> Result<AgentApprovalReceipt, ReviewerCallError> {
    run_review(
        router,
        &ReviewInputs {
            role: ReviewerRoleId::TestIntegrity,
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
