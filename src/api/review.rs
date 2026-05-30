//! Owner: TUI Control-Plane API + Web Forge BFF — review DTOs
//! Proof: `cargo nextest run -p jeryu --lib api`
//! Invariants: API contract types only; no I/O, no domain side effects.
//!
//! Source: FINAL spec §6.2 (referenced via `src/api/review.rs` slot in the
//! file matrix) plus §35 — the FINAL spec doesn't define review DTOs
//! directly, so we follow Codex/FINAL conventions and the `HostReviewThread`
//! shape in FINAL §6.8.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::repository::RepositoryId;

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct ReviewThread {
    pub id: String,
    pub repo: RepositoryId,
    pub mr_iid: String,
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_sha: Option<String>,
    pub comments: Vec<ReviewComment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct ReviewComment {
    pub id: String,
    pub author: String,
    pub body_markdown: String,
    /// Sanitized HTML rendered server-side via the markdown service. Optional
    /// because raw fetches may skip rendering; the UI re-sanitizes anyway.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_html: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<DateTime<Utc>>,
    /// Present when this comment is an inline ``` ```suggestion``` ``` block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<ReviewSuggestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<ReviewEvidence>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct ReviewSuggestion {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub original_text: String,
    pub suggested_text: String,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct ReviewEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_log_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// Body for `POST /api/v1/repos/{id}/merge-requests/{iid}/comments`.
#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct CreateReviewCommentRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub body_markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_sha: Option<String>,
}

/// Body for `POST /api/v1/repos/{id}/merge-requests/{iid}/reviews`.
#[derive(
    Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS,
)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct SubmitReviewRequest {
    pub verdict: ReviewVerdict,
    pub expected_head_sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thread_comments: Vec<CreateReviewCommentRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<ReviewEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_verdict_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ReviewVerdict::RequestChanges).unwrap(),
            "\"request_changes\""
        );
        assert_eq!(
            serde_json::to_string(&ReviewVerdict::Approve).unwrap(),
            "\"approve\""
        );
    }

    #[test]
    fn review_thread_roundtrip() {
        let thread = ReviewThread {
            id: "thread-1".into(),
            repo: RepositoryId {
                id: "repo-1".into(),
                host: "gitlab".into(),
                owner: "neverhuman".into(),
                name: "jeryu".into(),
            },
            mr_iid: "42".into(),
            resolved: false,
            file_path: Some("src/lib.rs".into()),
            line: Some(10),
            anchor_sha: Some("abc123".into()),
            comments: vec![ReviewComment {
                id: "c1".into(),
                author: "reviewer".into(),
                body_markdown: "Approved with reproducible evidence attached.".into(),
                body_html: Some("<p>Approved with reproducible evidence attached.</p>".into()),
                created_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
                edited_at: None,
                suggestion: None,
                evidence: Some(ReviewEvidence {
                    replay_command: Some("cargo test -p jeryu --lib merge::review".into()),
                    ci_log_path: Some("target/ci/review.log".into()),
                    receipt_path: Some("target/ci/review.receipt.json".into()),
                    digest: Some("sha256:0123456789abcdef".into()),
                }),
            }],
            created_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            updated_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_001, 0).unwrap(),
        };
        let json = serde_json::to_string(&thread).unwrap();
        let back: ReviewThread = serde_json::from_str(&json).unwrap();
        assert_eq!(back.comments.len(), 1);
        assert_eq!(
            back.comments[0].body_markdown,
            "Approved with reproducible evidence attached."
        );
    }

    #[test]
    fn submit_review_request_with_exact_head_sha() {
        let req = SubmitReviewRequest {
            verdict: ReviewVerdict::Approve,
            expected_head_sha: "abc123".into(),
            body_markdown: None,
            thread_comments: vec![],
            evidence: Some(ReviewEvidence {
                replay_command: Some("cargo test -p jeryu --lib api".into()),
                ci_log_path: Some("target/ci/review.log".into()),
                receipt_path: Some("target/ci/review.receipt.json".into()),
                digest: Some("sha256:feedface".into()),
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("expected_head_sha"));
        let back: SubmitReviewRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expected_head_sha, "abc123");
        assert!(matches!(back.verdict, ReviewVerdict::Approve));
        assert!(back.evidence.is_some());
    }
}
