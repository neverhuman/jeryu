//! Pull requests, reviews, and merge requests/results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    Draft,
    #[default]
    Open,
    ReadyForReview,
    BlockedByPolicy,
    BlockedByChecks,
    Approved,
    Queued,
    SpeculativeMergeTesting,
    Mergeable,
    Merged,
    Closed,
}

/// A single commit on a pull request branch.
///
/// Carries the data branch-protection enforcement needs that is not derivable
/// from the head ref alone: signature verification status (for
/// `require_signed_commits`) and parent count (for `required_linear_history`,
/// where a parent count > 1 marks a merge commit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequestCommit {
    pub sha: String,
    /// Whether the commit signature is verified (GitHub's `verification.verified`).
    #[serde(default)]
    pub verified: bool,
    /// Number of parents. `> 1` indicates a merge commit (non-linear history).
    #[serde(default = "default_commit_parents")]
    pub parents: u64,
}

pub fn default_commit_parents() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitBranchRef {
    pub label: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
}

impl GitBranchRef {
    pub fn new(label: impl Into<String>, sha: impl Into<String>) -> Self {
        let label = label.into();
        let ref_name = label
            .split_once(':')
            .map(|(_, branch)| branch.to_string())
            .unwrap_or_else(|| label.clone());
        Self {
            label,
            ref_name,
            sha: sha.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub issue_number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: PullRequestState,
    pub draft: bool,
    pub author: String,
    pub head: GitBranchRef,
    pub base: GitBranchRef,
    pub mergeable: bool,
    pub mergeable_state: String,
    pub merged: bool,
    pub merged_at: Option<DateTime<Utc>>,
    pub merge_commit_sha: Option<String>,
    /// Commits on the PR head, newest data first is not required. Used by
    /// `required_linear_history` and `require_signed_commits` enforcement.
    #[serde(default)]
    pub commits: Vec<PullRequestCommit>,
    /// Repository-relative paths changed by the PR. Used for CODEOWNERS
    /// owner-approval enforcement.
    #[serde(default)]
    pub changed_files: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreatePullRequestRequest {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub head: String,
    pub base: String,
    #[serde(default)]
    pub head_sha: Option<String>,
    #[serde(default)]
    pub base_sha: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub commits: Vec<PullRequestCommit>,
    #[serde(default)]
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UpdatePullRequestRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub state: Option<PullRequestState>,
    #[serde(default)]
    pub draft: Option<bool>,
    #[serde(default)]
    pub commits: Option<Vec<PullRequestCommit>>,
    #[serde(default)]
    pub changed_files: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewState {
    #[serde(rename = "APPROVED", alias = "APPROVE")]
    Approved,
    #[serde(rename = "CHANGES_REQUESTED", alias = "REQUEST_CHANGES")]
    ChangesRequested,
    #[serde(rename = "COMMENTED", alias = "COMMENT")]
    Commented,
    #[serde(rename = "DISMISSED", alias = "DISMISS")]
    Dismissed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Review {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    pub author: String,
    pub state: ReviewState,
    pub body: Option<String>,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewComment {
    pub id: Uuid,
    pub review_id: Uuid,
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    pub path: String,
    pub line: Option<u64>,
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ReviewCommentInput {
    pub path: String,
    #[serde(default)]
    pub line: Option<u64>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateReviewRequest {
    #[serde(default)]
    pub body: Option<String>,
    pub event: ReviewState,
    #[serde(default)]
    pub comments: Vec<ReviewCommentInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergePullRequestRequest {
    #[serde(default)]
    pub commit_title: Option<String>,
    #[serde(default)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default = "default_merge_method")]
    pub merge_method: String,
}

impl Default for MergePullRequestRequest {
    fn default() -> Self {
        Self {
            commit_title: None,
            commit_message: None,
            sha: None,
            merge_method: default_merge_method(),
        }
    }
}

pub fn default_merge_method() -> String {
    "merge".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeResult {
    pub sha: String,
    pub merged: bool,
    pub message: String,
}
