use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RepoKey {
    pub owner: String,
    pub repo: String,
}

impl RepoKey {
    pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateUserRequest {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Organization {
    pub id: Uuid,
    pub login: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateOrganizationRequest {
    pub login: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Team {
    pub id: Uuid,
    pub organization: String,
    pub slug: String,
    pub name: String,
    pub members: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateTeamRequest {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Repository {
    pub id: Uuid,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub description: Option<String>,
    pub default_branch: String,
    pub archived: bool,
    pub disabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateRepositoryRequest {
    pub name: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    #[default]
    Open,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Label {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateLabelRequest {
    pub name: String,
    #[serde(default = "default_label_color")]
    pub color: String,
    #[serde(default)]
    pub description: Option<String>,
}

pub fn default_label_color() -> String {
    "ededed".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Issue {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: IssueState,
    pub author: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone: Option<String>,
    pub comments: u64,
    pub pull_request: Option<PullRequestMarker>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequestMarker {
    pub url: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateIssueRequest {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<String>,
    #[serde(default)]
    pub milestone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UpdateIssueRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub state: Option<IssueState>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub assignees: Option<Vec<String>>,
    #[serde(default)]
    pub milestone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueComment {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub issue_number: u64,
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateCommentRequest {
    pub body: String,
}

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
#[serde(rename_all = "lowercase")]
pub enum CommitStatusState {
    Error,
    Failure,
    Pending,
    Success,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitStatus {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub sha: String,
    pub state: CommitStatusState,
    pub context: String,
    pub description: Option<String>,
    pub target_url: Option<String>,
    pub creator: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateCommitStatusRequest {
    pub state: CommitStatusState,
    #[serde(default = "default_status_context")]
    pub context: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub target_url: Option<String>,
}

pub fn default_status_context() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedStatus {
    pub state: CommitStatusState,
    pub total_count: usize,
    pub statuses: Vec<CommitStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckRunStatus {
    Queued,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckConclusion {
    ActionRequired,
    Cancelled,
    Failure,
    Neutral,
    Success,
    Skipped,
    Stale,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRunOutput {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRun {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub name: String,
    pub head_sha: String,
    pub status: CheckRunStatus,
    pub conclusion: Option<CheckConclusion>,
    pub details_url: Option<String>,
    pub output: Option<CheckRunOutput>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateCheckRunRequest {
    pub name: String,
    pub head_sha: String,
    #[serde(default)]
    pub status: Option<CheckRunStatus>,
    #[serde(default)]
    pub conclusion: Option<CheckConclusion>,
    #[serde(default)]
    pub details_url: Option<String>,
    #[serde(default)]
    pub output: Option<CheckRunOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRunList {
    pub total_count: usize,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckSuite {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub head_sha: String,
    pub status: CheckRunStatus,
    pub conclusion: Option<CheckConclusion>,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchProtectionRule {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub required_status_checks: Vec<String>,
    pub required_approving_review_count: u64,
    pub enforce_admins: bool,
    pub required_linear_history: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub require_signed_commits: bool,
    pub require_jankurai_proof: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SetBranchProtectionRequest {
    #[serde(default)]
    pub required_status_checks: Vec<String>,
    #[serde(default)]
    pub required_approving_review_count: u64,
    #[serde(default)]
    pub enforce_admins: bool,
    #[serde(default)]
    pub required_linear_history: bool,
    #[serde(default)]
    pub allow_force_pushes: bool,
    #[serde(default)]
    pub allow_deletions: bool,
    #[serde(default)]
    pub require_signed_commits: bool,
    #[serde(default)]
    pub require_jankurai_proof: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default = "default_webhook_content_type")]
    pub content_type: String,
    #[serde(default)]
    pub secret: Option<String>,
}

pub fn default_webhook_content_type() -> String {
    "json".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Webhook {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub name: String,
    pub active: bool,
    pub events: Vec<String>,
    pub config: WebhookConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateWebhookRequest {
    #[serde(default = "default_webhook_name")]
    pub name: String,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default = "default_webhook_events")]
    pub events: Vec<String>,
    pub config: WebhookConfig,
}

pub fn default_webhook_name() -> String {
    "web".to_string()
}

pub fn default_webhook_events() -> Vec<String> {
    vec!["push".to_string(), "pull_request".to_string()]
}

pub fn default_true() -> bool {
    true
}

impl Default for CreateWebhookRequest {
    fn default() -> Self {
        Self {
            name: default_webhook_name(),
            active: true,
            events: default_webhook_events(),
            config: WebhookConfig {
                url: String::new(),
                content_type: default_webhook_content_type(),
                secret: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub hook_id: Uuid,
    pub owner: String,
    pub repo: String,
    pub event: String,
    pub target_url: String,
    pub payload: Value,
    pub signature_256: Option<String>,
    pub delivered: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRun {
    pub id: Uuid,
    pub name: String,
    pub head_sha: String,
    pub status: CheckRunStatus,
    pub conclusion: Option<CheckConclusion>,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunList {
    pub total_count: usize,
    pub workflow_runs: Vec<WorkflowRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubAppInstallation {
    pub id: Uuid,
    pub account_login: String,
    pub repository_selection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallationList {
    pub total_count: usize,
    pub installations: Vec<GitHubAppInstallation>,
}
