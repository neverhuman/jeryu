//! Code-review contracts: threads, comments, suggestions, evidence, and the
//! review/comment submission request bodies.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::repository::RepositoryId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewEvidence {
    pub replay_command: Option<String>,
    pub ci_log_path: Option<String>,
    pub receipt_path: Option<String>,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewSuggestion {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub original_text: String,
    pub suggested_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewComment {
    pub id: String,
    pub author: String,
    pub body_markdown: String,
    /// Sanitized HTML rendered server-side via the markdown service. Optional
    /// because raw fetches may skip rendering; the UI re-sanitizes anyway.
    pub body_html: Option<String>,
    pub created_at: String,
    pub edited_at: Option<String>,
    /// Present when this comment is an inline ``` ```suggestion``` ``` block.
    pub suggestion: Option<ReviewSuggestion>,
    pub evidence: Option<ReviewEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewThread {
    pub id: String,
    pub repo: RepositoryId,
    pub pr_number: u32,
    pub resolved: bool,
    pub file_path: Option<String>,
    pub line: Option<u32>,
    pub anchor_sha: Option<String>,
    pub comments: Vec<ReviewComment>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

/// Body for `POST /api/v1/repos/{id}/pulls/{number}/comments`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreateReviewCommentRequest {
    pub thread_id: Option<String>,
    pub body_markdown: String,
    pub file_path: Option<String>,
    pub line: Option<u32>,
    pub anchor_sha: Option<String>,
}

/// Body for `POST /api/v1/repos/{id}/pulls/{number}/reviews`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmitReviewRequest {
    pub verdict: ReviewVerdict,
    pub expected_head_sha: String,
    pub body_markdown: Option<String>,
    pub thread_comments: Vec<CreateReviewCommentRequest>,
    pub evidence: Option<ReviewEvidence>,
}

/// Body for a targeted self-dismissal; the review UUID is a route parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DismissPullReviewRequest {
    pub expected_head_sha: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::DismissPullReviewRequest;

    #[test]
    fn dismissal_contract_requires_head_and_reason() {
        for missing in [
            r#"{"reason":"withdraw verdict"}"#,
            r#"{"expected_head_sha":"abc"}"#,
        ] {
            assert!(serde_json::from_str::<DismissPullReviewRequest>(missing).is_err());
        }
        let request = DismissPullReviewRequest {
            expected_head_sha: "a".repeat(40),
            reason: "withdraw verdict".to_string(),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["expected_head_sha"], "a".repeat(40));
        assert_eq!(json["reason"], "withdraw verdict");
    }
}
