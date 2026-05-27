//! Owner: GitLab host adapter — W-H-04 (MR + reviews) + W-H-05 (merge with SHA).
//! Proof: `cargo nextest run -p jeryu --lib git_host::gitlab`
//! Invariants:
//!   - All write calls go through `?sha=` or `{ sha }` body fences (Tip1 Law 4).
//!   - 409 responses surface as `HostError::Conflict` so the service layer
//!     can map to `merge_sha_stale` per §35.9 item 7.
//!   - URLs are built via `encode_project_path`; no string-formatting of repo
//!     slug into a URL segment elsewhere (§35.1.10 path-safety).
//!
//! Layered onto `GitLabClient` (defined in `gitlab.rs`) via `#[path]` so the
//! merge/review methods live next to their request/response types without
//! inflating the main adapter file.

use chrono::Utc;
use reqwest::{Method, StatusCode};

use crate::git_host::{
    GitHost, HostError, HostMergeInput, HostMergeResult, HostPipeline, HostReview,
    HostReviewComment, HostReviewCommentInput, HostReviewThread, HostSubmitReviewInput, MrApproval,
    Page, PageResult, RepoRef,
};

use super::GitLabClient;
use super::gitlab_browse::encode_project_path;
use super::gitlab_helpers::{map_error, map_reqwest};
use super::gitlab_types::{
    GitLabCreateDiscussionReq, GitLabDiscussion, GitLabDiscussionCreatePosition, GitLabMergeMrReq,
    GitLabMergeMrResp, GitLabResolveDiscussionReq,
};

impl GitLabClient {
    /// `GET /projects/:id/merge_requests/:iid/discussions` — list every
    /// discussion (thread) with its notes; W-H-04.
    pub(super) async fn gl_list_discussions(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
    ) -> Result<Vec<GitLabDiscussion>, HostError> {
        let enc = encode_project_path(repo);
        self.inner
            .get_paginated_json(&format!(
                "/projects/{enc}/merge_requests/{mr_iid}/discussions"
            ))
            .await
            .map_err(map_error)
    }

    /// `POST /projects/:id/merge_requests/:iid/discussions` — open a
    /// positional (line-anchored) discussion. The `position` block is required
    /// for inline review comments per the GitLab Notes API.
    pub(super) async fn gl_create_discussion(
        &self,
        input: HostReviewCommentInput<'_>,
        base_sha: &str,
        head_sha: &str,
    ) -> Result<GitLabDiscussion, HostError> {
        let enc = encode_project_path(input.repo);
        let path = input.path;
        let position = GitLabDiscussionCreatePosition {
            position_type: "text",
            base_sha,
            start_sha: base_sha,
            head_sha,
            new_path: path,
            old_path: path,
            new_line: input.line,
        };
        let body = GitLabCreateDiscussionReq {
            body: input.body,
            position: Some(position),
        };
        self.post_json(
            &format!(
                "/projects/{enc}/merge_requests/{mr_iid}/discussions",
                mr_iid = input.mr_iid
            ),
            &body,
        )
        .await
    }

    /// Toggle a discussion's resolved flag. Maps to
    /// `PUT /projects/:id/merge_requests/:iid/discussions/:discussion_id`.
    /// Exposed to the `merge` domain so the BFF can patch threads without
    /// adding the toggle to the `GitHost` trait surface (it has no parallel
    /// in the GitHub adapter today).
    pub async fn resolve_discussion(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
        discussion_id: &str,
        resolved: bool,
    ) -> Result<(), HostError> {
        let enc = encode_project_path(repo);
        let url = self.inner.api_url(&format!(
            "/projects/{enc}/merge_requests/{mr_iid}/discussions/{discussion_id}"
        ));
        let body = GitLabResolveDiscussionReq { resolved };
        self.inner
            .authed_request_url(Method::PUT, url)
            .map_err(map_error)?
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest)?
            .error_for_status()
            .map_err(map_reqwest)?;
        Ok(())
    }

    /// Merge a MR with the exact-SHA fence in body. `PUT /projects/:id/merge_requests/:iid/merge`.
    /// Returns `HostError::Conflict` on 409 so the service layer can surface
    /// `merge_sha_stale` per §35.9 item 7.
    pub(super) async fn gl_merge_mr(
        &self,
        input: HostMergeInput<'_>,
    ) -> Result<HostMergeResult, HostError> {
        let enc = encode_project_path(input.repo);
        let url = self.inner.api_url(&format!(
            "/projects/{enc}/merge_requests/{iid}/merge",
            iid = input.mr_iid
        ));
        let squash = matches!(input.method, "squash");
        let body = GitLabMergeMrReq {
            sha: input.expected_head_sha,
            merge_commit_message: input.commit_message,
            squash_commit_message: input.commit_message,
            squash,
        };
        let resp = self
            .inner
            .authed_request_url(Method::PUT, url)
            .map_err(map_error)?
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest)?;
        let status = resp.status();
        if status == StatusCode::CONFLICT {
            let body = resp.text().await.unwrap_or_default();
            return Err(HostError::Conflict(format!(
                "gitlab merge 409: {body}"
            )));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(HostError::Permanent(format!(
                "gitlab merge http {status}: {body}"
            )));
        }
        let parsed: GitLabMergeMrResp = resp.json().await.map_err(map_reqwest)?;
        let merged = parsed
            .state
            .as_deref()
            .map(|s| s == "merged")
            .unwrap_or(true);
        let sha = parsed
            .merge_commit_sha
            .or(parsed.squash_commit_sha)
            .or(parsed.sha);
        Ok(HostMergeResult {
            merged,
            sha,
            url: parsed.web_url,
        })
    }

    /// `POST /projects/:id/merge_requests/:iid/unapprove` — used by
    /// "request_changes" submission flows. Not strictly the same as
    /// "request changes" but the closest GitLab semantic.
    pub(super) async fn gl_unapprove_mr(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
    ) -> Result<(), HostError> {
        let enc = encode_project_path(repo);
        let url = self.inner.api_url(&format!(
            "/projects/{enc}/merge_requests/{mr_iid}/unapprove"
        ));
        self.inner
            .authed_request_url(Method::POST, url)
            .map_err(map_error)?
            .send()
            .await
            .map_err(map_reqwest)?
            .error_for_status()
            .map_err(map_reqwest)?;
        Ok(())
    }

    /// `GET /projects/:id/pipelines` — used by the Merge Passport gate #7
    /// (Required CI green) and the CI cockpit.
    pub(super) async fn gl_list_pipelines(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostPipeline>, HostError> {
        let enc = encode_project_path(repo);
        let mut query = String::new();
        if let Some(r) = ref_name {
            query.push_str(&format!("ref={}", urlencoding::encode(r)));
        }
        let path = if query.is_empty() {
            format!("/projects/{enc}/pipelines")
        } else {
            format!("/projects/{enc}/pipelines?{query}")
        };
        let per_page = page.per_page.clamp(1, 100);
        let cursor = page.cursor.unwrap_or_else(|| "1".to_string());
        let joiner = if path.contains('?') { '&' } else { '?' };
        let url_with_page = self
            .inner
            .api_url(&format!("{path}{joiner}per_page={per_page}&page={cursor}"));
        let resp = self
            .inner
            .authed_request_url(Method::GET, url_with_page)
            .map_err(map_error)?
            .send()
            .await
            .map_err(map_reqwest)?
            .error_for_status()
            .map_err(map_reqwest)?;
        let next_cursor = resp
            .headers()
            .get("x-next-page")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let body = resp.text().await.map_err(map_reqwest)?;
        #[derive(serde::Deserialize)]
        struct GitLabPipeline {
            id: i64,
            #[serde(default)]
            r#ref: Option<String>,
            #[serde(default)]
            sha: Option<String>,
            #[serde(default)]
            status: Option<String>,
            #[serde(default)]
            created_at: Option<chrono::DateTime<chrono::Utc>>,
            #[serde(default)]
            updated_at: Option<chrono::DateTime<chrono::Utc>>,
            #[serde(default)]
            web_url: Option<String>,
        }
        let items: Vec<GitLabPipeline> = serde_json::from_str(&body).map_err(|e| {
            HostError::Permanent(format!("deserialize pipelines: {e}; body: {body}"))
        })?;
        let now = Utc::now();
        let pipelines = items
            .into_iter()
            .map(|p| HostPipeline {
                id: p.id.to_string(),
                ref_name: p.r#ref.unwrap_or_default(),
                sha: p.sha.unwrap_or_default(),
                status: p.status.unwrap_or_else(|| "unknown".to_string()),
                created_at: p.created_at.unwrap_or(now),
                updated_at: p.updated_at.unwrap_or(now),
                web_url: p.web_url,
            })
            .collect();
        Ok(PageResult {
            items: pipelines,
            next_cursor,
            total: None,
        })
    }
}

/// Convert a `GitLabDiscussion` into the canonical `HostReviewThread` shape.
pub(super) fn discussion_to_thread(d: GitLabDiscussion) -> HostReviewThread {
    let resolved = d
        .notes
        .iter()
        .filter(|n| n.resolvable)
        .all(|n| n.resolved);
    let (path, line) = d
        .notes
        .iter()
        .find_map(|n| n.position.as_ref().map(|p| (p.new_path.clone(), p.new_line)))
        .unwrap_or((None, None));
    let comments: Vec<HostReviewComment> = d
        .notes
        .into_iter()
        .filter(|n| !n.system)
        .map(|n| HostReviewComment {
            id: n.id.to_string(),
            author: n
                .author
                .map(|a| a.username)
                .unwrap_or_else(|| "unknown".to_string()),
            body: n.body,
            created_at: n.created_at,
        })
        .collect();
    HostReviewThread {
        id: d.id,
        path,
        line,
        resolved,
        comments,
    }
}

/// Trait: lets us add the `GitHost` overrides for W-H-04/05 in this file
/// without colliding with the `gitlab.rs` impl block.
#[async_trait::async_trait]
pub(super) trait GitlabMerge: Send + Sync {
    async fn list_review_threads(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
    ) -> Result<Vec<HostReviewThread>, HostError>;

    async fn create_review_comment(
        &self,
        input: HostReviewCommentInput<'_>,
    ) -> Result<HostReviewComment, HostError>;

    async fn submit_review(
        &self,
        input: HostSubmitReviewInput<'_>,
    ) -> Result<HostReview, HostError>;

    async fn merge_mr(&self, input: HostMergeInput<'_>) -> Result<HostMergeResult, HostError>;

    async fn list_pipelines(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostPipeline>, HostError>;
}

#[async_trait::async_trait]
impl GitlabMerge for GitLabClient {
    async fn list_review_threads(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
    ) -> Result<Vec<HostReviewThread>, HostError> {
        let discussions = self.gl_list_discussions(repo, mr_iid).await?;
        Ok(discussions.into_iter().map(discussion_to_thread).collect())
    }

    async fn create_review_comment(
        &self,
        input: HostReviewCommentInput<'_>,
    ) -> Result<HostReviewComment, HostError> {
        // We need base_sha + head_sha for the position block; fetch live PR
        // state. This is one extra hop but keeps the comment exactly anchored
        // to the current diff_refs the user is reviewing against (Tip1 Law 4).
        let live = <Self as GitHost>::get_pr_state(self, input.repo, input.mr_iid).await?;
        if live.target_branch_sha.is_empty() {
            return Err(HostError::Permanent(
                "create_review_comment: live target_branch_sha empty".into(),
            ));
        }
        if live.head_sha.is_empty() {
            return Err(HostError::Permanent(
                "create_review_comment: live head_sha empty".into(),
            ));
        }
        let base_sha = live.target_branch_sha.clone();
        let head_sha = live.head_sha.clone();
        let fallback_body = input.body.to_string();
        let discussion = self
            .gl_create_discussion(input, &base_sha, &head_sha)
            .await?;
        // Return the first note in the new discussion (GitLab returns the
        // discussion with its primary note already attached).
        let now = Utc::now();
        if let Some(note) = discussion.notes.into_iter().next() {
            Ok(HostReviewComment {
                id: note.id.to_string(),
                author: note
                    .author
                    .map(|a| a.username)
                    .unwrap_or_else(|| "unknown".into()),
                body: note.body,
                created_at: note.created_at,
            })
        } else {
            // Shouldn't happen; build a synthetic comment from the input.
            Ok(HostReviewComment {
                id: discussion.id,
                author: "unknown".into(),
                body: fallback_body,
                created_at: now,
            })
        }
    }

    async fn submit_review(
        &self,
        input: HostSubmitReviewInput<'_>,
    ) -> Result<HostReview, HostError> {
        match input.event {
            "approve" => {
                let receipt_digest = format!(
                    "review-{}-{}-{}",
                    input.repo.slug(),
                    input.mr_iid,
                    input.head_sha
                );
                let id = self
                    .approve_mr(MrApproval {
                        repo: input.repo,
                        mr_iid: input.mr_iid,
                        head_sha: input.head_sha,
                        agent_id: "web-reviewer",
                        receipt_digest: &receipt_digest,
                        dry_run: false,
                    })
                    .await?;
                if let Some(body) = input.body {
                    if !body.trim().is_empty() {
                        self.post_mr_comment(input.repo, input.mr_iid, body).await?;
                    }
                }
                Ok(HostReview {
                    id,
                    state: "APPROVED".into(),
                    url: None,
                })
            }
            "request_changes" => {
                let body = input
                    .body
                    .filter(|b| !b.trim().is_empty())
                    .unwrap_or("Changes requested.");
                let id = self
                    .post_mr_comment(input.repo, input.mr_iid, body)
                    .await?;
                // Try to unapprove (no-op if the viewer hadn't approved
                // earlier — surface as transient and ignore).
                let _ = self.gl_unapprove_mr(input.repo, input.mr_iid).await;
                Ok(HostReview {
                    id,
                    state: "CHANGES_REQUESTED".into(),
                    url: None,
                })
            }
            _ => {
                // "comment" and any unexpected event string both post a plain note.
                let body = input.body.unwrap_or("");
                let id = self
                    .post_mr_comment(input.repo, input.mr_iid, body)
                    .await?;
                Ok(HostReview {
                    id,
                    state: "COMMENTED".into(),
                    url: None,
                })
            }
        }
    }

    async fn merge_mr(&self, input: HostMergeInput<'_>) -> Result<HostMergeResult, HostError> {
        self.gl_merge_mr(input).await
    }

    async fn list_pipelines(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostPipeline>, HostError> {
        self.gl_list_pipelines(repo, ref_name, page).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_host::HostReviewComment;
    use super::super::gitlab_types::{
        GitLabDiscussion, GitLabDiscussionNote, GitLabDiscussionNoteAuthor,
        GitLabDiscussionNotePosition,
    };

    fn note(
        id: i64,
        body: &str,
        resolvable: bool,
        resolved: bool,
        position: Option<GitLabDiscussionNotePosition>,
        system: bool,
    ) -> GitLabDiscussionNote {
        GitLabDiscussionNote {
            id,
            body: body.into(),
            author: Some(GitLabDiscussionNoteAuthor {
                username: "alice".into(),
            }),
            created_at: chrono::Utc::now(),
            resolvable,
            resolved,
            position,
            system,
        }
    }

    #[test]
    fn discussion_to_thread_collects_unresolved_notes() {
        let d = GitLabDiscussion {
            id: "disc-1".into(),
            notes: vec![
                note(
                    1,
                    "first",
                    true,
                    false,
                    Some(GitLabDiscussionNotePosition {
                        new_path: Some("src/lib.rs".into()),
                        old_path: Some("src/lib.rs".into()),
                        new_line: Some(42),
                        old_line: None,
                        base_sha: None,
                        head_sha: None,
                        start_sha: None,
                    }),
                    false,
                ),
                note(2, "second", true, false, None, false),
                note(3, "system marker", false, false, None, true),
            ],
        };
        let t = discussion_to_thread(d);
        assert_eq!(t.id, "disc-1");
        assert!(!t.resolved);
        assert_eq!(t.path.as_deref(), Some("src/lib.rs"));
        assert_eq!(t.line, Some(42));
        assert_eq!(t.comments.len(), 2, "system notes are filtered out");
        let _: &HostReviewComment = &t.comments[0]; // type guard
    }

    #[test]
    fn discussion_to_thread_marks_resolved_when_all_resolvable_notes_resolved() {
        let d = GitLabDiscussion {
            id: "disc-2".into(),
            notes: vec![
                note(1, "a", true, true, None, false),
                note(2, "b", true, true, None, false),
            ],
        };
        let t = discussion_to_thread(d);
        assert!(t.resolved);
    }

    #[test]
    fn discussion_to_thread_with_no_resolvable_notes_reports_resolved() {
        // An empty / unresolvable thread is treated as "no blocker" so the
        // Merge Passport doesn't trip on system notes.
        let d = GitLabDiscussion {
            id: "disc-3".into(),
            notes: vec![note(1, "system", false, false, None, true)],
        };
        let t = discussion_to_thread(d);
        assert!(t.resolved);
    }
}
