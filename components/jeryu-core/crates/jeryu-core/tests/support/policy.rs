//! Historical policy-algorithm tests intentionally supply advisory rows. Core's
//! authenticated boundary is exercised separately and never consumes this helper.
use jeryu_core::*;

pub fn evaluate_advisory(
    core: &ForgeCore,
    owner: &str,
    repo: &str,
    number: u64,
    sha: Option<&str>,
) -> Result<BranchProtectionEvaluation> {
    let pr = core.get_pull_request(owner, repo, number)?;
    let rule = core
        .get_branch_protection(owner, repo, &pr.base.ref_name)
        .ok();
    let codeowners = core.get_codeowners(owner, repo).ok();
    let reviews = core.list_reviews(owner, repo, number)?;
    let statuses = core.combined_status(owner, repo, &pr.head.sha)?.statuses;
    let checks = core
        .list_check_runs(owner, repo, Some(&pr.head.sha))?
        .check_runs;
    Ok(evaluate_branch_protection_with(
        &pr,
        rule.as_ref(),
        &reviews,
        &statuses,
        &checks,
        sha,
        EvaluationContext {
            codeowners: codeowners.as_deref(),
            actor_is_admin: false,
            jankurai_proof_mandatory: false,
        },
    ))
}
