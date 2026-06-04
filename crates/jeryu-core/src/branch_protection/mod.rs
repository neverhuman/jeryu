//! Branch-protection evaluation: merge gates, ref-operation bars, CODEOWNERS.

mod codeowners;
mod evaluate;
mod types;

pub use evaluate::{evaluate_branch_protection_with, evaluate_ref_operation};
pub use types::{
    BranchProtectionEvaluation, EvaluationContext, MergeBlocker, RefOperation, RefOperationBlocker,
    RefOperationEvaluation,
};

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::model::{BranchProtectionRule, GitBranchRef, PullRequest, PullRequestState};

    fn pr() -> PullRequest {
        PullRequest {
            id: Uuid::new_v4(),
            owner: "acme".to_string(),
            repo: "jeryu".to_string(),
            number: 1,
            issue_number: 1,
            title: "change".to_string(),
            body: None,
            state: PullRequestState::Open,
            draft: false,
            author: "dev".to_string(),
            source_repository: "acme/jeryu".to_string(),
            head: GitBranchRef::new("feature", "abc"),
            base: GitBranchRef::new("main", "base"),
            mergeable: false,
            mergeable_state: "unknown".to_string(),
            merged: false,
            merged_at: None,
            merge_commit_sha: None,
            commits: Vec::new(),
            changed_files: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn branch_protection_blocks_missing_review_and_status() {
        let rule = BranchProtectionRule {
            owner: "acme".to_string(),
            repo: "jeryu".to_string(),
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

        let eval = evaluate_branch_protection_with(
            &pr(),
            Some(&rule),
            &[],
            &[],
            &[],
            None,
            EvaluationContext::default(),
        );
        assert!(!eval.mergeable);
        assert_eq!(eval.blockers.len(), 2);
    }

    #[test]
    fn branch_protection_applies_equally_to_owner_and_fork_prs_no_bypass() {
        // Negative authorization proof for the enforce_admins boundary above:
        // a fork / non-owner PR (source_repository != owner/repo) must NOT bypass
        // branch protection. The review/status/admin gates apply identically to
        // owner and fork PRs — source_repository confers no merge authority.
        let rule = BranchProtectionRule {
            owner: "acme".to_string(),
            repo: "jeryu".to_string(),
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

        let owner_pr = pr(); // source_repository == "acme/jeryu" (an owner PR)
        let mut fork_pr = pr();
        fork_pr.author = "outside-contributor".to_string();
        fork_pr.source_repository = "outside-contributor/jeryu-fork".to_string();

        let owner_eval = evaluate_branch_protection_with(
            &owner_pr,
            Some(&rule),
            &[],
            &[],
            &[],
            None,
            EvaluationContext::default(),
        );
        let fork_eval = evaluate_branch_protection_with(
            &fork_pr,
            Some(&rule),
            &[],
            &[],
            &[],
            None,
            EvaluationContext::default(),
        );

        assert!(!owner_eval.mergeable);
        assert!(
            !fork_eval.mergeable,
            "a fork / non-owner PR must not bypass branch protection"
        );
        assert_eq!(
            fork_eval.blockers.len(),
            owner_eval.blockers.len(),
            "owner and fork PRs hit branch protection identically — source_repository grants no bypass"
        );
    }
}
