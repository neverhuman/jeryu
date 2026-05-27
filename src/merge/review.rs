//! Owner: Web Forge BFF — ReviewService (W-B-12).
//! Proof: `cargo nextest run -p jeryu --lib merge::review`
//! Invariants:
//!   - `submit_review` ALWAYS calls `verify_head_sha` so review verdicts
//!     never apply to a stale head (Tip1 Law 4).
//!   - Every mutation writes audit + emits a WS event.
//!   - Inline `suggestion` parsing is a thin extractor; the suggestion engine
//!     proper lives in W-B-15.

use std::sync::Arc;

use chrono::Utc;
use jeryu::api::repository::RepositoryId;
use jeryu::api::review::{
    CreateReviewCommentRequest, ReviewComment, ReviewSuggestion, ReviewThread, ReviewVerdict,
    SubmitReviewRequest,
};
use jeryu::api::websocket::WebEvent;
use jeryu::git_host::{
    GitHost, GitLabClient, HostReviewCommentInput, HostSubmitReviewInput,
};
use jeryu::web_events::WebEventBus;
use serde_json::json;
use sqlx::AnyPool;

use crate::repos::models::RepoId;
use crate::repos::service::host_to_api_error;
use crate::web::audit::{RiskTier, write_audit};
use crate::web::error::ApiError;

use super::guards;

pub struct ReviewService {
    gitlab: Arc<GitLabClient>,
    event_bus: Arc<WebEventBus>,
    db_pool: AnyPool,
}

impl ReviewService {
    pub fn new(gitlab: Arc<GitLabClient>, event_bus: Arc<WebEventBus>, db_pool: AnyPool) -> Self {
        Self {
            gitlab,
            event_bus,
            db_pool,
        }
    }

    /// `GET /api/v1/repos/{repo_id}/merge-requests/{iid}/threads`
    pub async fn list_threads(
        &self,
        repo_id: &str,
        iid: &str,
    ) -> Result<Vec<ReviewThread>, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        let host_threads = GitHost::list_review_threads(self.gitlab.as_ref(), &repo, iid)
            .await
            .map_err(host_to_api_error)?;
        let repo_dto = RepositoryId::from(&parsed);
        Ok(host_threads
            .into_iter()
            .map(|t| host_thread_to_api(&repo_dto, iid, t))
            .collect())
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/threads` — opens a
    /// new (positional) discussion. Returns the new thread shape.
    pub async fn create_thread(
        &self,
        repo_id: &str,
        iid: &str,
        req: CreateReviewCommentRequest,
        actor: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ReviewThread, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        let file_path = req
            .file_path
            .as_deref()
            .ok_or_else(|| ApiError::ValidationFailed("file_path is required for thread".into()))?;
        let line = req
            .line
            .ok_or_else(|| ApiError::ValidationFailed("line is required for thread".into()))?;
        let host_comment = GitHost::create_review_comment(
            self.gitlab.as_ref(),
            HostReviewCommentInput {
                repo: &repo,
                mr_iid: iid,
                body: &req.body_markdown,
                path: file_path,
                line,
                side: "right",
            },
        )
        .await
        .map_err(host_to_api_error)?;
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.thread.create",
            &format!("mr:{repo_id}/{iid}"),
            RiskTier::Low,
            json!({
                "iid": iid,
                "file_path": file_path,
                "line": line,
                "idempotency_key": idempotency_key,
            }),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.thread.create)"
            );
        }
        let repo_dto = RepositoryId::from(&parsed);
        let thread = ReviewThread {
            id: host_comment.id.clone(),
            repo: repo_dto.clone(),
            mr_iid: iid.to_string(),
            resolved: false,
            file_path: Some(file_path.to_string()),
            line: Some(line),
            anchor_sha: req.anchor_sha.clone(),
            comments: vec![ReviewComment {
                id: host_comment.id,
                author: host_comment.author,
                body_markdown: host_comment.body.clone(),
                body_html: None,
                created_at: host_comment.created_at,
                edited_at: None,
                suggestion: parse_suggestion(&host_comment.body, file_path, line),
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.publish(
            &parsed,
            iid,
            "mr.thread.created",
            "review thread created",
            json!({"iid": iid, "thread_id": thread.id, "file_path": file_path, "line": line}),
        );
        Ok(thread)
    }

    /// `PATCH /api/v1/repos/{repo_id}/merge-requests/{iid}/threads/{thread_id}`
    /// — flip the `resolved` flag. Calls the GitLab adapter to toggle the
    /// remote discussion, then re-fetches threads so the response reflects
    /// the host's authoritative state.
    pub async fn resolve_thread(
        &self,
        repo_id: &str,
        iid: &str,
        thread_id: &str,
        resolved: bool,
        actor: &str,
    ) -> Result<ReviewThread, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        self.gitlab
            .resolve_discussion(&repo, iid, thread_id, resolved)
            .await
            .map_err(host_to_api_error)?;
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.thread.resolve",
            &format!("mr:{repo_id}/{iid}/thread:{thread_id}"),
            RiskTier::Low,
            json!({"iid": iid, "thread_id": thread_id, "resolved": resolved}),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.thread.resolve)"
            );
        }
        let all = self.list_threads(repo_id, iid).await?;
        let found = all
            .into_iter()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| ApiError::NotFound(format!("thread {thread_id}")))?;
        self.publish(
            &parsed,
            iid,
            if resolved {
                "mr.thread.resolved"
            } else {
                "mr.thread.unresolved"
            },
            "review thread resolution toggled",
            json!({"iid": iid, "thread_id": thread_id, "resolved": resolved}),
        );
        Ok(found)
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/comments` — add a
    /// follow-up comment. When `thread_id` is set, the comment threads onto
    /// an existing discussion; otherwise it's a top-level MR note.
    pub async fn create_comment(
        &self,
        repo_id: &str,
        iid: &str,
        req: CreateReviewCommentRequest,
        actor: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ReviewComment, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        let id = GitHost::post_mr_comment(self.gitlab.as_ref(), &repo, iid, &req.body_markdown)
            .await
            .map_err(host_to_api_error)?;
        let comment = ReviewComment {
            id: id.clone(),
            author: actor.to_string(),
            body_markdown: req.body_markdown.clone(),
            body_html: None,
            created_at: Utc::now(),
            edited_at: None,
            suggestion: req.file_path.as_deref().and_then(|p| {
                req.line.and_then(|l| parse_suggestion(&req.body_markdown, p, l))
            }),
        };
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.comment.create",
            &format!("mr:{repo_id}/{iid}"),
            RiskTier::Low,
            json!({
                "iid": iid,
                "thread_id": req.thread_id,
                "comment_id": id,
                "idempotency_key": idempotency_key,
            }),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.comment.create)"
            );
        }
        self.publish(
            &parsed,
            iid,
            "mr.comment.created",
            "review comment posted",
            json!({"iid": iid, "comment_id": id, "thread_id": req.thread_id}),
        );
        Ok(comment)
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/reviews` — submit
    /// the verdict with the exact-SHA fence.
    pub async fn submit_review(
        &self,
        repo_id: &str,
        iid: &str,
        req: SubmitReviewRequest,
        actor: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ReviewSubmissionResult, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        let bound = guards::verify_head_sha(
            self.gitlab.as_ref(),
            &repo,
            iid,
            &req.expected_head_sha,
        )
        .await?;
        let event = match req.verdict {
            ReviewVerdict::Approve => "approve",
            ReviewVerdict::RequestChanges => "request_changes",
            ReviewVerdict::Comment => "comment",
        };
        // Post thread-anchored comments first so they exist when the verdict
        // lands (matches the GitLab review surface ordering).
        for tc in &req.thread_comments {
            if let (Some(path), Some(line)) = (tc.file_path.as_deref(), tc.line) {
                let _ = GitHost::create_review_comment(
                    self.gitlab.as_ref(),
                    HostReviewCommentInput {
                        repo: &repo,
                        mr_iid: iid,
                        body: &tc.body_markdown,
                        path,
                        line,
                        side: "right",
                    },
                )
                .await
                .map_err(host_to_api_error)?;
            }
        }
        let review = GitHost::submit_review(
            self.gitlab.as_ref(),
            HostSubmitReviewInput {
                repo: &repo,
                mr_iid: iid,
                head_sha: &bound,
                event,
                body: req.body_markdown.as_deref(),
            },
        )
        .await
        .map_err(host_to_api_error)?;
        let kind = match req.verdict {
            ReviewVerdict::Approve => "mr.review.approved",
            ReviewVerdict::RequestChanges => "mr.review.changes_requested",
            ReviewVerdict::Comment => "mr.review.commented",
        };
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.review.submit",
            &format!("mr:{repo_id}/{iid}"),
            match req.verdict {
                ReviewVerdict::Approve => RiskTier::High,
                _ => RiskTier::Medium,
            },
            json!({
                "iid": iid,
                "verdict": event,
                "head_sha": bound,
                "review_id": review.id,
                "idempotency_key": idempotency_key,
            }),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.review.submit)"
            );
        }
        self.publish(
            &parsed,
            iid,
            kind,
            "review verdict submitted",
            json!({"iid": iid, "verdict": event, "review_id": review.id, "head_sha": bound}),
        );
        Ok(ReviewSubmissionResult {
            review_id: review.id,
            state: review.state,
            head_sha: bound,
        })
    }

    fn publish(
        &self,
        repo: &RepoId,
        iid: &str,
        kind: &str,
        summary: &str,
        payload: serde_json::Value,
    ) {
        let scope = format!("mr.{repo}/{iid}", repo = repo.encode());
        let entity = format!("mr:{repo}/{iid}", repo = repo.encode());
        let evt = WebEvent {
            seq: 0,
            timestamp: Utc::now(),
            scope,
            kind: kind.to_string(),
            entity,
            summary: summary.to_string(),
            payload,
        };
        let _ = self.event_bus.publish(evt);
    }
}

#[derive(Debug, Clone)]
pub struct ReviewSubmissionResult {
    pub review_id: String,
    pub state: String,
    pub head_sha: String,
}

fn parse_repo_id(raw: &str) -> Result<RepoId, ApiError> {
    RepoId::parse(raw).ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {raw}")))
}

fn host_thread_to_api(
    repo: &RepositoryId,
    iid: &str,
    t: jeryu::git_host::HostReviewThread,
) -> ReviewThread {
    let now = Utc::now();
    let mut earliest = now;
    let mut latest = now;
    let comments: Vec<ReviewComment> = t
        .comments
        .into_iter()
        .map(|c| {
            if c.created_at < earliest {
                earliest = c.created_at;
            }
            if c.created_at > latest {
                latest = c.created_at;
            }
            ReviewComment {
                id: c.id,
                author: c.author,
                body_markdown: c.body.clone(),
                body_html: None,
                created_at: c.created_at,
                edited_at: None,
                suggestion: t
                    .path
                    .as_deref()
                    .zip(t.line)
                    .and_then(|(p, l)| parse_suggestion(&c.body, p, l)),
            }
        })
        .collect();
    ReviewThread {
        id: t.id,
        repo: repo.clone(),
        mr_iid: iid.to_string(),
        resolved: t.resolved,
        file_path: t.path,
        line: t.line,
        anchor_sha: None,
        comments,
        created_at: earliest,
        updated_at: latest,
    }
}

/// Lightweight inline-suggestion extractor — picks the first
/// ```suggestion fenced block out of `body`. The full diff-anchored
/// suggestion engine lands with W-B-15.
fn parse_suggestion(body: &str, file_path: &str, line: u32) -> Option<ReviewSuggestion> {
    let needle = "```suggestion";
    let start = body.find(needle)?;
    let rest = &body[start + needle.len()..];
    // skip optional language tag and newline after fence open
    let rest = rest.trim_start_matches('\n');
    let end = rest.find("```")?;
    let suggested = rest[..end].trim_end_matches('\n').to_string();
    Some(ReviewSuggestion {
        file_path: file_path.to_string(),
        start_line: line,
        end_line: line,
        original_text: String::new(),
        suggested_text: suggested,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_suggestion_extracts_fenced_block() {
        let body = "Please fix:\n```suggestion\nlet x = 42;\n```\nthanks!";
        let s = parse_suggestion(body, "src/lib.rs", 10).expect("parsed");
        assert_eq!(s.file_path, "src/lib.rs");
        assert_eq!(s.start_line, 10);
        assert_eq!(s.suggested_text, "let x = 42;");
    }

    #[test]
    fn parse_suggestion_returns_none_when_no_block() {
        assert!(parse_suggestion("no suggestion here", "x", 1).is_none());
    }

    #[test]
    fn host_thread_to_api_carries_metadata() {
        use jeryu::git_host::{HostReviewComment, HostReviewThread};
        let repo = RepositoryId {
            id: "gitlab/o/r".into(),
            host: "gitlab".into(),
            owner: "o".into(),
            name: "r".into(),
        };
        let t = HostReviewThread {
            id: "disc-1".into(),
            path: Some("src/lib.rs".into()),
            line: Some(7),
            resolved: false,
            comments: vec![HostReviewComment {
                id: "1".into(),
                author: "alice".into(),
                body: "look here".into(),
                created_at: Utc::now(),
            }],
        };
        let api = host_thread_to_api(&repo, "42", t);
        assert_eq!(api.mr_iid, "42");
        assert_eq!(api.file_path.as_deref(), Some("src/lib.rs"));
        assert_eq!(api.line, Some(7));
        assert_eq!(api.comments.len(), 1);
    }
}
