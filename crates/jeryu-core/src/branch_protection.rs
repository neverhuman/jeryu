use serde::{Deserialize, Serialize};

use crate::model::{
    BranchProtectionRule, CheckConclusion, CheckRun, CheckRunStatus, CommitStatus,
    CommitStatusState, PullRequest, Review, ReviewState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MergeBlocker {
    DraftPullRequest,
    MissingReview {
        required: u64,
        approved: u64,
    },
    MissingStatusCheck {
        context: String,
    },
    FailedStatusCheck {
        context: String,
    },
    JankuraiProofRequired,
    ShaMismatch {
        expected: String,
        actual: String,
    },
    /// `required_linear_history` is on and the PR contains a merge commit
    /// (a commit with more than one parent).
    NonLinearHistory {
        sha: String,
    },
    /// `require_signed_commits` is on and at least one commit is unsigned /
    /// unverified.
    UnsignedCommits {
        unsigned: Vec<String>,
    },
    /// CODEOWNERS-owned paths changed but a required code owner has not approved.
    MissingCodeOwnerReview {
        paths: Vec<String>,
        owners: Vec<String>,
    },
}

/// A ref-mutating operation that branch protection can forbid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefOperation {
    /// A non-fast-forward (force) push to the protected ref.
    ForcePush,
    /// Deletion of the protected ref.
    Delete,
}

/// Reasons a ref operation (force-push / deletion) was rejected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RefOperationBlocker {
    /// `allow_force_pushes = false` forbids force-pushing the protected ref.
    ForcePushNotAllowed,
    /// `allow_deletions = false` forbids deleting the protected ref.
    DeletionNotAllowed,
}

/// Result of evaluating a ref-mutating operation against branch protection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefOperationEvaluation {
    pub allowed: bool,
    pub blockers: Vec<RefOperationBlocker>,
}

/// Extra inputs branch-protection enforcement needs beyond the PR's own state.
#[derive(Debug, Default, Clone, Copy)]
pub struct EvaluationContext<'a> {
    /// Raw contents of the repository's CODEOWNERS file, if one exists.
    pub codeowners: Option<&'a str>,
    /// Whether the acting principal is a repository admin. When the rule's
    /// `enforce_admins` is false, an admin actor bypasses configurable
    /// protections (matching GitHub's "Include administrators" toggle).
    pub actor_is_admin: bool,
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

/// Minimal CODEOWNERS model: ordered `(glob, owners)` rules. As on GitHub the
/// *last* matching rule for a path wins.
#[derive(Debug, Default)]
struct CodeOwners {
    rules: Vec<(String, Vec<String>)>,
}

impl CodeOwners {
    fn parse(contents: &str) -> Self {
        let mut rules = Vec::new();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(pattern) = parts.next() else {
                continue;
            };
            let owners: Vec<String> = parts
                .map(|owner| owner.trim_start_matches('@').to_string())
                .filter(|owner| !owner.is_empty())
                .collect();
            rules.push((pattern.to_string(), owners));
        }
        Self { rules }
    }

    /// Owners for a path: the owners of the last matching rule (GitHub order).
    /// A path with no matching rule is unowned — an explicit empty owner set,
    /// not a swallowed error.
    fn owners_for(&self, path: &str) -> Vec<String> {
        match self
            .rules
            .iter()
            .rev()
            .find(|(pattern, _)| glob_match(pattern, path))
        {
            Some((_, owners)) => owners.clone(),
            None => Vec::new(),
        }
    }
}

/// Minimal CODEOWNERS path-glob match. Supports a leading `*` extension glob
/// (e.g. `*.rs`), directory prefixes (`docs/`, `/src/`), and a `/**` suffix.
fn glob_match(pattern: &str, path: &str) -> bool {
    let path = path.trim_start_matches('/');

    // `*` matches everything.
    if pattern == "*" {
        return true;
    }

    // Extension / filename glob: `*.rs`, `*.md`.
    if let Some(suffix) = pattern.strip_prefix('*') {
        return path.ends_with(suffix);
    }

    let pat = pattern.trim_start_matches('/');

    // Recursive directory glob: `src/**` matches anything under `src/`.
    if let Some(prefix) = pat.strip_suffix("/**") {
        let prefix = prefix.trim_end_matches('/');
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }

    // Directory prefix: `docs/` matches everything under `docs/`.
    if let Some(prefix) = pat.strip_suffix('/') {
        return path.starts_with(&format!("{prefix}/")) || path == prefix;
    }

    // A bare directory name (no trailing slash) also covers its contents, the
    // way GitHub treats `docs` as `docs/`.
    if path == pat || path.starts_with(&format!("{pat}/")) {
        return true;
    }

    false
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
            repo: "jeryu".to_string(),
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
}
