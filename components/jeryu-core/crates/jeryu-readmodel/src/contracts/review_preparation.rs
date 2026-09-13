//! Browser projection of the complete Core review challenge. Additional Core
//! snapshot fields remain on the wire and in its immutable review evidence.
//! This read model neither authenticates the challenge nor authorizes a review.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PreparePullReviewRequest {
    pub expected_head_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullReviewObservedRef {
    pub reference: String,
    pub commit_sha: String,
    pub tree_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullReviewGitObservation {
    pub source: PullReviewObservedRef,
    pub destination: PullReviewObservedRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullReviewActor {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullReviewSnapshot {
    pub repository_id: String,
    #[ts(type = "number")]
    pub pull_number: u64,
    pub git: PullReviewGitObservation,
    pub policy_revision: String,
    pub reviewer: PullReviewActor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullReviewChallenge {
    pub id: String,
    pub nonce: String,
    pub expires_at: String,
    pub snapshot_sha256: String,
    pub snapshot: PullReviewSnapshot,
    pub merge_qualified: bool,
    pub blockers: Vec<String>,
}

/// Complete body for the credential-bound approval endpoint. The server also
/// diagnoses incomplete historical requests, but a new client must bind a nonce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullApproveRequest {
    pub expected_head_sha: String,
    pub challenge_id: String,
    pub nonce: String,
    #[ts(optional)]
    pub body_markdown: Option<String>,
}
