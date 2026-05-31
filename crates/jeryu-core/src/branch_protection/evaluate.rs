//! Branch-protection and ref-operation evaluation against PR/rule state.

use crate::model::{
    BranchProtectionRule, CheckConclusion, CheckRun, CheckRunStatus, CommitStatus,
    CommitStatusState, PullRequest, Review, ReviewState,
};

use super::codeowners::CodeOwners;
use super::types::{
    BranchProtectionEvaluation, EvaluationContext, MergeBlocker, RefOperation, RefOperationBlocker,
    RefOperationEvaluation,
};

#[allow(clippy::too_many_arguments)]
pub fn evaluate_branch_protection_with(
    pr: &PullRequest,
    protection: Option<&BranchProtectionRule>,
    reviews: &[Review],
    statuses: &[CommitStatus],
    check_runs: &[CheckRun],
    requested_sha: Option<&str>,
    context: EvaluationContext<'_>,
) -> BranchProtectionEvaluation {
    let mut blockers = Vec::new();

    // Intrinsic gates apply regardless of any protection rule (and even to
    // admins): a stale SHA and a draft PR are never mergeable on GitHub.
    if let Some(requested_sha) = requested_sha
        && requested_sha != pr.head.sha
    {
        blockers.push(MergeBlocker::ShaMismatch {
            expected: pr.head.sha.clone(),
            actual: requested_sha.to_string(),
        });
    }

    if pr.draft {
        blockers.push(MergeBlocker::DraftPullRequest);
    }

    let Some(rule) = protection else {
        return BranchProtectionEvaluation::from_blockers(blockers);
    };

    // GitHub's "Include administrators" toggle: when `enforce_admins` is off, an
    // admin actor bypasses the configurable rule gates below (the intrinsic
    // gates above still apply). When it is on, the rules bind everyone.
    if context.actor_is_admin && !rule.enforce_admins {
        return BranchProtectionEvaluation::from_blockers(blockers);
    }

    if rule.required_approving_review_count > 0 {
        let approved = reviews
            .iter()
            .filter(|review| review.state == ReviewState::Approved)
            .count() as u64;
        if approved < rule.required_approving_review_count {
            blockers.push(MergeBlocker::MissingReview {
                required: rule.required_approving_review_count,
                approved,
            });
        }
    }

    for required_context in &rule.required_status_checks {
        match required_context_state(required_context, statuses, check_runs) {
            RequiredContextState::Satisfied => {}
            RequiredContextState::Missing => blockers.push(MergeBlocker::MissingStatusCheck {
                context: required_context.clone(),
            }),
            RequiredContextState::Failed => blockers.push(MergeBlocker::FailedStatusCheck {
                context: required_context.clone(),
            }),
        }
    }

    if rule.require_jankurai_proof {
        match required_context_state("jankurai/proof", statuses, check_runs) {
            RequiredContextState::Satisfied => {}
            RequiredContextState::Missing | RequiredContextState::Failed => {
                blockers.push(MergeBlocker::JankuraiProofRequired);
            }
        }
    }

    if rule.required_linear_history
        && let Some(merge_commit) = pr.commits.iter().find(|commit| commit.parents > 1)
    {
        blockers.push(MergeBlocker::NonLinearHistory {
            sha: merge_commit.sha.clone(),
        });
    }

    if rule.require_signed_commits {
        let unsigned: Vec<String> = pr
            .commits
            .iter()
            .filter(|commit| !commit.verified)
            .map(|commit| commit.sha.clone())
            .collect();
        if !unsigned.is_empty() {
            blockers.push(MergeBlocker::UnsignedCommits { unsigned });
        }
    }

    if let Some(codeowners) = context.codeowners {
        let owners = CodeOwners::parse(codeowners);
        let approvers: std::collections::BTreeSet<&str> = reviews
            .iter()
            .filter(|review| review.state == ReviewState::Approved)
            .map(|review| review.author.as_str())
            .collect();

        // A changed path is unsatisfied when it has code owners but none of them
        // has approved (GitHub: any single owner of the path suffices).
        let mut unsatisfied_paths: Vec<String> = Vec::new();
        let mut required_owners: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for path in &pr.changed_files {
            let path_owners = owners.owners_for(path);
            if path_owners.is_empty() {
                continue;
            }
            let satisfied = path_owners
                .iter()
                .any(|owner| approvers.contains(owner.as_str()));
            if !satisfied {
                unsatisfied_paths.push(path.clone());
                required_owners.extend(path_owners);
            }
        }
        if !unsatisfied_paths.is_empty() {
            unsatisfied_paths.sort();
            unsatisfied_paths.dedup();
            blockers.push(MergeBlocker::MissingCodeOwnerReview {
                paths: unsatisfied_paths,
                owners: required_owners.into_iter().collect(),
            });
        }
    }

    BranchProtectionEvaluation::from_blockers(blockers)
}

/// Evaluates a ref-mutating operation (force-push / deletion) against the
/// branch's protection rule. With no rule, all operations are allowed.
pub fn evaluate_ref_operation(
    operation: RefOperation,
    protection: Option<&BranchProtectionRule>,
    context: EvaluationContext<'_>,
) -> RefOperationEvaluation {
    let mut blockers = Vec::new();
    if let Some(rule) = protection {
        // `enforce_admins` off lets admins bypass the force-push / deletion bars.
        let bypass = context.actor_is_admin && !rule.enforce_admins;
        if !bypass {
            match operation {
                RefOperation::ForcePush if !rule.allow_force_pushes => {
                    blockers.push(RefOperationBlocker::ForcePushNotAllowed);
                }
                RefOperation::Delete if !rule.allow_deletions => {
                    blockers.push(RefOperationBlocker::DeletionNotAllowed);
                }
                _ => {}
            }
        }
    }
    RefOperationEvaluation {
        allowed: blockers.is_empty(),
        blockers,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredContextState {
    Satisfied,
    Missing,
    Failed,
}

fn required_context_state(
    context: &str,
    statuses: &[CommitStatus],
    check_runs: &[CheckRun],
) -> RequiredContextState {
    let status_match = statuses
        .iter()
        .filter(|status| status.context == context)
        .max_by_key(|status| status.updated_at);

    if let Some(status) = status_match {
        return if status.state == CommitStatusState::Success {
            RequiredContextState::Satisfied
        } else {
            RequiredContextState::Failed
        };
    }

    let check_match = check_runs
        .iter()
        .filter(|check| check.name == context)
        .max_by_key(|check| check.completed_at.or(Some(check.started_at)));

    if let Some(check) = check_match {
        return if check.status == CheckRunStatus::Completed
            && check.conclusion == Some(CheckConclusion::Success)
        {
            RequiredContextState::Satisfied
        } else {
            RequiredContextState::Failed
        };
    }

    RequiredContextState::Missing
}
