use serde::{Deserialize, Serialize};

use crate::model::{
    BranchProtectionRule, CheckConclusion, CheckRun, CheckRunStatus, CommitStatus,
    CommitStatusState, PullRequest, Review, ReviewState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MergeBlocker {
    DraftPullRequest,
    MissingReview { required: u64, approved: u64 },
    MissingStatusCheck { context: String },
    FailedStatusCheck { context: String },
    JankuraiProofRequired,
    ShaMismatch { expected: String, actual: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchProtectionEvaluation {
    pub mergeable: bool,
    pub state: String,
    pub blockers: Vec<MergeBlocker>,
}

impl BranchProtectionEvaluation {
    pub fn pass() -> Self {
        Self {
            mergeable: true,
            state: "clean".to_string(),
            blockers: Vec::new(),
        }
    }

    pub fn from_blockers(blockers: Vec<MergeBlocker>) -> Self {
        Self {
            mergeable: blockers.is_empty(),
            state: if blockers.is_empty() {
                "clean".to_string()
            } else {
                "blocked".to_string()
            },
            blockers,
        }
    }
}

pub fn evaluate_branch_protection(
    pr: &PullRequest,
    protection: Option<&BranchProtectionRule>,
    reviews: &[Review],
    statuses: &[CommitStatus],
    check_runs: &[CheckRun],
    requested_sha: Option<&str>,
) -> BranchProtectionEvaluation {
    let mut blockers = Vec::new();

    if let Some(requested_sha) = requested_sha {
        if requested_sha != pr.head.sha {
            blockers.push(MergeBlocker::ShaMismatch {
                expected: pr.head.sha.clone(),
                actual: requested_sha.to_string(),
            });
        }
    }

    if pr.draft {
        blockers.push(MergeBlocker::DraftPullRequest);
    }

    let Some(rule) = protection else {
        return BranchProtectionEvaluation::from_blockers(blockers);
    };

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

    BranchProtectionEvaluation::from_blockers(blockers)
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

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::model::{GitBranchRef, PullRequestState};

    fn pr() -> PullRequest {
        PullRequest {
            id: Uuid::new_v4(),
            owner: "acme".to_string(),
            repo: "nitro".to_string(),
            number: 1,
            issue_number: 1,
            title: "change".to_string(),
            body: None,
            state: PullRequestState::Open,
            draft: false,
            author: "dev".to_string(),
            head: GitBranchRef::new("feature", "abc"),
            base: GitBranchRef::new("main", "base"),
            mergeable: false,
            mergeable_state: "unknown".to_string(),
            merged: false,
            merged_at: None,
            merge_commit_sha: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn branch_protection_blocks_missing_review_and_status() {
        let rule = BranchProtectionRule {
            owner: "acme".to_string(),
            repo: "nitro".to_string(),
            branch: "main".to_string(),
            required_status_checks: vec!["ci/fast".to_string()],
            required_approving_review_count: 1,
            enforce_admins: true,
            required_linear_history: true,
            allow_force_pushes: false,
            allow_deletions: false,
            require_signed_commits: false,
            require_jankurai_proof: false,
            updated_at: Utc::now(),
        };

        let eval = evaluate_branch_protection(&pr(), Some(&rule), &[], &[], &[], None);
        assert!(!eval.mergeable);
        assert_eq!(eval.blockers.len(), 2);
    }
}
