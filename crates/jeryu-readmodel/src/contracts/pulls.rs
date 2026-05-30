//! Pull-request and issue summary/detail contracts plus the Merge Passport
//! verdict consumed by the SPA's PR and merge-gate views.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::action::AvailableAction;
use super::entity::EntityHandle;
use super::repository::RepositoryId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IssueSummary {
    pub repo: RepositoryId,
    pub number: u32,
    pub title: String,
    pub state: IssueState,
    pub labels: Vec<String>,
    pub author: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum PullRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Mergeability {
    pub level: String,
    pub can_merge: bool,
    pub reason: Option<String>,
    pub exact_head_sha: String,
    pub required_gate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewPosture {
    pub required_approvals: u32,
    pub approvals: u32,
    pub changes_requested: u32,
    pub unresolved_threads: u32,
    pub user_review_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CheckPosture {
    pub total: u32,
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentPosture {
    pub active_sessions: u32,
    pub proposed_patches: u32,
    pub evidence_packets: u32,
    pub blockers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum MergePassportStatus {
    Pass,
    Blocked,
}

/// One blocker entry in the Merge Passport. `code` aligns with the §35.2.4
/// canonical gate list (e.g. `passport_blocked_approvals`,
/// `passport_blocked_policy_sha`) so the UI can target explanations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergePassportBlocker {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

/// Canonical Merge Passport verdict (FINAL spec §6.7 + §35.2.4 12-gate list).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergePassport {
    pub status: MergePassportStatus,
    pub head_sha: String,
    pub blockers: Vec<MergePassportBlocker>,
    pub evaluated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullRequestSummary {
    pub repo: RepositoryId,
    pub number: u32,
    #[ts(type = "{ kind: string; id: string }")]
    pub entity: EntityHandle,
    pub title: String,
    pub author: String,
    pub head_ref: String,
    pub base_ref: String,
    pub head_sha: String,
    pub base_sha: String,
    pub state: PullRequestState,
    pub draft: bool,
    pub mergeable: Mergeability,
    pub review: ReviewPosture,
    pub checks: CheckPosture,
    pub agents: AgentPosture,
    pub labels: Vec<String>,
    pub updated_at: String,
    /// Stable hash of the most recent Merge Passport evaluation. Per §35.1.14
    /// the backend persists `passport_hash` on `web_pull_requests` after each
    /// Passport recompute so we can detect re-evaluation drift across replays.
    /// `None` when the Passport has not been computed yet (e.g. brand-new PR).
    pub passport_hash: Option<String>,
    #[ts(type = "Array<{ action_id: string; label: string; risk: string | null }>")]
    pub available_actions: Vec<AvailableAction>,
}

/// Full PR view returned by `GET /api/v1/repos/{repo_id}/pulls/{number}`.
///
/// Carries the same posture summary fields plus the Merge Passport verdict
/// (per §35.2.4) so the UI's `MergeGatePanel` can render gate-by-gate detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullRequestDetail {
    pub summary: PullRequestSummary,
    pub description: Option<String>,
    pub merge_passport: MergePassport,
    /// `passport_hash` mirrors `summary.passport_hash` for ergonomics so the
    /// UI doesn't need to dive into `summary` to display the verdict identity.
    pub passport_hash: Option<String>,
}
