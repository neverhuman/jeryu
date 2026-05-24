//! Live GitLab `GitHost` adapter.
//!
//! This adapter intentionally maps the generic `GitHost` surface onto GitLab
//! REST primitives without introducing GitHub-shaped checks. The single visible
//! gate is a commit status named `vibegate/merge-passport`.

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::git_host::{
    ChangedFileDiff, CheckRun, CheckRunResult, CheckStatus, GitHost, HostError, HostIdentity,
    MrApproval, PrDiff, PrLiveState, PrSummary, RepoRef, VIBEGATE_MERGE_PASSPORT_CHECK_NAME,
};
use crate::gitlab_client::GitlabClient;

#[path = "gitlab_types.rs"]
mod gitlab_types;
pub use gitlab_types::GitLabProtectedBranch;
use gitlab_types::*;

#[derive(Clone)]
pub struct GitLabClient {
    inner: GitlabClient,
}

impl GitLabClient {
    pub fn new(inner: GitlabClient) -> Self {
        Self { inner }
    }

    pub fn from_env() -> Result<Self, HostError> {
        let url = match std::env::var("GITLAB_URL") {
            Ok(value) if !value.is_empty() => value,
            _ => match std::env::var("CI_SERVER_URL") {
                Ok(value) if !value.is_empty() => value,
                _ => "http://127.0.0.1:8929".to_string(),
            },
        };
        let token = match std::env::var("GITLAB_PAT") {
            Ok(value) if !value.is_empty() => value,
            _ => match std::env::var("GITLAB_TOKEN") {
                Ok(value) if !value.is_empty() => value,
                _ => match std::env::var("PRIVATE_TOKEN") {
                    Ok(value) if !value.is_empty() => value,
                    _ => return Err(HostError::Auth),
                },
            },
        };
        Ok(Self::new(GitlabClient::new(&url, Some(token))))
    }

    pub async fn from_jeryu_env_or_repair() -> Result<Self, HostError> {
        let auth = crate::gitlab_auth::resolve_or_repair_default()
            .await
            .map_err(|err| {
                if err.to_string().contains("token not found") {
                    HostError::Auth
                } else {
                    HostError::Permanent(err.to_string())
                }
            })?;
        Ok(Self::new(GitlabClient::new(&auth.url, Some(auth.token))))
    }

    fn project_ref(repo: &RepoRef) -> String {
        urlencoding::encode(&repo.slug()).into_owned()
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, HostError> {
        self.inner
            .api_get_json(self.inner.api_url(path))
            .await
            .map_err(map_error)
    }

    async fn post_json<Req: Serialize, Resp: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp, HostError> {
        self.inner
            .api_post_json(self.inner.api_url(path), body)
            .await
            .map_err(map_error)
    }

    async fn post_note(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
        body: &str,
    ) -> Result<GitLabNote, HostError> {
        let project = Self::project_ref(repo);
        self.post_json(
            &format!("/projects/{project}/merge_requests/{mr_iid}/notes"),
            &GitLabNoteReq { body },
        )
        .await
    }

    pub async fn post_merge_passport_status(
        &self,
        repo: &RepoRef,
        head_sha: &str,
        status: CheckStatus,
        summary: &str,
        details_url: Option<&str>,
    ) -> Result<CheckRunResult, HostError> {
        self.post_check_run(CheckRun {
            repo,
            head_sha,
            name: VIBEGATE_MERGE_PASSPORT_CHECK_NAME,
            status,
            summary,
            details_url,
            output_text: None,
        })
        .await
    }

    pub async fn list_protected_branches(
        &self,
        repo: &RepoRef,
    ) -> Result<Vec<GitLabProtectedBranch>, HostError> {
        let project = Self::project_ref(repo);
        self.inner
            .get_paginated_json(&format!("/projects/{project}/protected_branches"))
            .await
            .map_err(map_error)
    }
}

#[async_trait]
impl GitHost for GitLabClient {
    fn id(&self) -> &str {
        "gitlab"
    }

    async fn ping_user(&self) -> Result<HostIdentity, HostError> {
        let user: GitLabUser = self.get_json("/user").await?;
        Ok(HostIdentity {
            login: user.username,
            host: "gitlab".into(),
        })
    }

    async fn post_check_run(&self, input: CheckRun<'_>) -> Result<CheckRunResult, HostError> {
        let project = Self::project_ref(input.repo);
        let state = gitlab_status(input.status);
        let mut body = GitLabCommitStatusReq {
            state,
            name: input.name,
            target_url: input.details_url,
            description: Some(input.summary),
        };
        if body.name.is_empty() {
            body.name = VIBEGATE_MERGE_PASSPORT_CHECK_NAME;
        }
        let status: GitLabCommitStatus = self
            .post_json(
                &format!("/projects/{project}/statuses/{}", input.head_sha),
                &body,
            )
            .await?;
        Ok(CheckRunResult {
            id: status.id.to_string(),
            url: status.target_url,
        })
    }

    async fn post_mr_comment(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
        body: &str,
    ) -> Result<String, HostError> {
        let note = self.post_note(repo, mr_iid, body).await?;
        Ok(note.id.to_string())
    }

    async fn approve_mr(&self, input: MrApproval<'_>) -> Result<String, HostError> {
        if input.dry_run {
            return Ok(format!(
                "dry-run:gitlab-approve:{}!{}@{}:{}",
                input.repo.slug(),
                input.mr_iid,
                input.head_sha,
                input.receipt_digest
            ));
        }
        let project = Self::project_ref(input.repo);
        let url = self.inner.api_url(&format!(
            "/projects/{project}/merge_requests/{}/approve?sha={}",
            input.mr_iid, input.head_sha
        ));
        let resp = self
            .inner
            .authed_request_url(Method::POST, url)
            .map_err(map_error)?
            .send()
            .await
            .map_err(map_reqwest)?
            .error_for_status()
            .map_err(map_reqwest)?;
        let approved: GitLabApprovalResp = resp.json().await.map_err(map_reqwest)?;
        Ok(approved.id.to_string())
    }

    async fn list_open_prs(&self, repo: &RepoRef) -> Result<Vec<PrSummary>, HostError> {
        let project = Self::project_ref(repo);
        let mrs: Vec<GitLabMergeRequest> = self
            .inner
            .get_paginated_json(&format!(
                "/projects/{project}/merge_requests?state=opened&scope=all"
            ))
            .await
            .map_err(map_error)?;
        Ok(mrs.into_iter().map(pr_summary_from_mr).collect())
    }

    async fn get_pr_state(&self, repo: &RepoRef, mr_iid: &str) -> Result<PrLiveState, HostError> {
        let project = Self::project_ref(repo);
        let mr: GitLabMergeRequest = self
            .get_json(&format!("/projects/{project}/merge_requests/{mr_iid}"))
            .await?;
        let target_branch_sha = target_branch_sha_from_mr(&mr);
        let head_sha = head_sha_from_mr(&mr);
        let target_policy_sha = self
            .fetch_target_policy_sha(repo, &mr.target_branch)
            .await
            .unwrap_or(None);
        Ok(PrLiveState {
            mr_iid: mr.iid.to_string(),
            head_sha,
            target_branch: mr.target_branch,
            target_branch_sha,
            target_policy_sha,
            fetched_at: Utc::now(),
        })
    }

    async fn fetch_pr_diff(&self, repo: &RepoRef, mr_iid: &str) -> Result<PrDiff, HostError> {
        let project = Self::project_ref(repo);
        let mr: GitLabMergeRequest = self
            .get_json(&format!(
                "/projects/{project}/merge_requests/{mr_iid}/changes"
            ))
            .await?;
        let head_sha = head_sha_from_mr(&mr);
        let base_sha = target_branch_sha_from_mr(&mr);
        let changed_files = match mr.changes {
            Some(changes) => changes,
            None => Vec::new(),
        }
        .into_iter()
        .map(|c| {
            let (lines_added, lines_removed) = count_diff_lines(&c.diff);
            ChangedFileDiff {
                path: change_path(&c),
                lines_added,
                lines_removed,
                hunks: if c.diff.is_empty() {
                    Vec::new()
                } else {
                    vec![c.diff]
                },
            }
        })
        .collect();
        Ok(PrDiff {
            repo: repo.slug(),
            mr_iid: mr.iid.to_string(),
            head_sha,
            base_sha,
            changed_files,
            fetched_at: Utc::now(),
        })
    }

    async fn fetch_target_policy_sha(
        &self,
        repo: &RepoRef,
        target_branch: &str,
    ) -> Result<Option<String>, HostError> {
        let project = Self::project_ref(repo);
        let branch = urlencoding::encode(target_branch);
        let tree: Vec<GitLabTreeEntry> = match self
            .get_json(&format!(
                "/projects/{project}/repository/tree?path=.jeryu/autonomy/policies&ref={branch}&per_page=100"
            ))
            .await
        {
            Ok(tree) => tree,
            Err(HostError::Permanent(msg)) if msg.contains("404") => return Ok(None),
            Err(err) => return Err(err),
        };
        let mut names: Vec<String> = tree
            .into_iter()
            .filter(|e| e.kind == "blob" && e.name.ends_with(".yml"))
            .map(|e| e.name)
            .collect();
        names.sort();
        if names.is_empty() {
            return Ok(None);
        }
        let mut joined = String::new();
        for name in names {
            let policy_path = format!(".jeryu/autonomy/policies/{name}");
            let path = urlencoding::encode(&policy_path);
            let file: GitLabRepositoryFile = self
                .get_json(&format!(
                    "/projects/{project}/repository/files/{path}?ref={branch}"
                ))
                .await?;
            let decoded = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                file.content.replace('\n', ""),
            )
            .map_err(|e| HostError::Permanent(format!("decode policy file {name}: {e}")))?;
            joined.push_str(&String::from_utf8_lossy(&decoded));
            if !joined.ends_with('\n') {
                joined.push('\n');
            }
        }
        Ok(Some(crate::autonomy::signing::sha256_digest(
            joined.as_bytes(),
        )))
    }
}

fn pr_summary_from_mr(mr: GitLabMergeRequest) -> PrSummary {
    let head_sha = head_sha_from_mr(&mr);
    PrSummary {
        mr_iid: mr.iid.to_string(),
        head_sha,
        target_branch: mr.target_branch,
        author: match mr.author {
            Some(author) if !author.username.is_empty() => author.username,
            _ => "unknown".into(),
        },
        title: mr.title,
        draft: mr.draft || mr.work_in_progress,
        labels: mr.labels,
    }
}

fn target_branch_sha_from_mr(mr: &GitLabMergeRequest) -> String {
    let Some(refs) = mr.diff_refs.as_ref() else {
        return String::new();
    };
    if let Some(base) = non_empty_clone(refs.base_sha.as_ref()) {
        return base;
    }
    if let Some(start) = non_empty_clone(refs.start_sha.as_ref()) {
        return start;
    }
    String::new()
}

fn head_sha_from_mr(mr: &GitLabMergeRequest) -> String {
    if let Some(sha) = non_empty_clone(mr.sha.as_ref()) {
        return sha;
    }
    let Some(refs) = mr.diff_refs.as_ref() else {
        return String::new();
    };
    match non_empty_clone(refs.head_sha.as_ref()) {
        Some(sha) => sha,
        None => String::new(),
    }
}

fn change_path(change: &GitLabChange) -> String {
    if let Some(path) = non_empty_clone(change.new_path.as_ref()) {
        return path;
    }
    match non_empty_clone(change.old_path.as_ref()) {
        Some(path) => path,
        None => String::new(),
    }
}

fn non_empty_clone(value: Option<&String>) -> Option<String> {
    let value = value?;
    if value.is_empty() {
        None
    } else {
        Some(value.clone())
    }
}

fn gitlab_status(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Queued => "pending",
        CheckStatus::InProgress => "running",
        CheckStatus::Success => "success",
        CheckStatus::Failure | CheckStatus::ActionRequired => "failed",
        CheckStatus::Neutral => "success",
    }
}

fn count_diff_lines(diff: &str) -> (u32, u32) {
    let mut added = 0;
    let mut removed = 0;
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    (added, removed)
}

fn map_error(err: anyhow::Error) -> HostError {
    let msg = err.to_string();
    if msg.contains("401") || msg.contains("403") || msg.contains("no PAT configured") {
        HostError::Auth
    } else if msg.contains("429") {
        HostError::RateLimited {
            retry_after_ms: 30_000,
        }
    } else if msg.contains("502") || msg.contains("503") || msg.contains("504") {
        HostError::Transient(msg)
    } else {
        HostError::Permanent(msg)
    }
}

fn map_reqwest(err: reqwest::Error) -> HostError {
    if err.status() == Some(reqwest::StatusCode::UNAUTHORIZED)
        || err.status() == Some(reqwest::StatusCode::FORBIDDEN)
    {
        HostError::Auth
    } else if err.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
        HostError::RateLimited {
            retry_after_ms: 30_000,
        }
    } else if err.is_timeout() || err.is_connect() {
        HostError::Transient(err.to_string())
    } else {
        HostError::Permanent(err.to_string())
    }
}

#[cfg(test)]
#[path = "gitlab_tests.rs"]
mod tests;
