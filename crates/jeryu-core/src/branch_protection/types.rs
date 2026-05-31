use serde::{Deserialize, Serialize};

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
