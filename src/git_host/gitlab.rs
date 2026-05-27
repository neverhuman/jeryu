//! Live GitLab `GitHost` adapter.
//!
//! This adapter intentionally maps the generic `GitHost` surface onto GitLab
//! REST primitives without introducing GitHub-shaped checks. The single visible
//! gate is a commit status named `vibegate/merge-passport`.

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Method;

use crate::git_host::{
    ChangedFileDiff, CheckRun, CheckRunResult, CreateHostRepository, GitHost, HostBlob, HostCommit,
    HostCompare, HostError, HostIdentity, HostJob, HostJobLog, HostMergeInput, HostMergeResult,
    HostPipeline, HostRef, HostRepository, HostRepositorySettingsPatch, HostReview,
    HostReviewComment, HostReviewCommentInput, HostReviewThread, HostSubmitReviewInput,
    HostTreeEntry, MrApproval, Page, PageResult, PrDiff, PrLiveState, PrSummary, RepoRef,
    VIBEGATE_MERGE_PASSPORT_CHECK_NAME,
};
use crate::gitlab_client::GitlabClient;

#[path = "gitlab_types.rs"]
mod gitlab_types;
pub use gitlab_types::GitLabProtectedBranch;
use gitlab_types::*;

#[path = "gitlab_helpers.rs"]
mod gitlab_helpers;
use gitlab_helpers::*;

// Sibling module that adds an inherent `impl GitLabClient` block. Loaded via
// `#[path]` so the type lives in gitlab.rs and the constructors / HTTP
// helpers live next to it without inflating this file.
#[path = "gitlab_client.rs"]
mod gitlab_client_impl;

// W-H-02 / W-H-03 helpers. The `GitlabBrowse` sealed trait inside provides
// the impl bodies, which we re-dispatch through the canonical `GitHost`
// trait below.
#[path = "gitlab_browse.rs"]
mod gitlab_browse;
use gitlab_browse::GitlabBrowse;

// W-H-04 / W-H-05 helpers. `GitlabMerge` is the sealed counterpart trait for
// MR + review + merge methods.
#[path = "gitlab_merge.rs"]
mod gitlab_merge;
use gitlab_merge::GitlabMerge;

#[derive(Clone)]
pub struct GitLabClient {
    pub(super) inner: GitlabClient,
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

    // ── W-H-02: repo discovery + browse ───────────────────────────────

    async fn list_repositories(
        &self,
        owner: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostRepository>, HostError> {
        <Self as GitlabBrowse>::list_repositories(self, owner, page).await
    }

    async fn get_repository(&self, repo: &RepoRef) -> Result<HostRepository, HostError> {
        <Self as GitlabBrowse>::get_repository(self, repo).await
    }

    async fn list_refs(&self, repo: &RepoRef) -> Result<Vec<HostRef>, HostError> {
        <Self as GitlabBrowse>::list_refs(self, repo).await
    }

    async fn list_tree(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<Vec<HostTreeEntry>, HostError> {
        <Self as GitlabBrowse>::list_tree(self, repo, ref_name, path).await
    }

    async fn get_blob(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<HostBlob, HostError> {
        <Self as GitlabBrowse>::get_blob(self, repo, ref_name, path).await
    }

    async fn get_readme(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
    ) -> Result<HostBlob, HostError> {
        <Self as GitlabBrowse>::get_readme(self, repo, ref_name).await
    }

    async fn compare_refs(
        &self,
        repo: &RepoRef,
        base: &str,
        head: &str,
    ) -> Result<HostCompare, HostError> {
        <Self as GitlabBrowse>::compare_refs(self, repo, base, head).await
    }

    async fn list_commits(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostCommit>, HostError> {
        <Self as GitlabBrowse>::list_commits(self, repo, ref_name, path, page).await
    }

    // ── W-H-03: repo create / update ──────────────────────────────────

    async fn create_repository(
        &self,
        input: CreateHostRepository<'_>,
    ) -> Result<HostRepository, HostError> {
        <Self as GitlabBrowse>::create_repository(self, input).await
    }

    async fn update_repository_settings(
        &self,
        repo: &RepoRef,
        patch: HostRepositorySettingsPatch<'_>,
    ) -> Result<HostRepository, HostError> {
        <Self as GitlabBrowse>::update_repository_settings(self, repo, patch).await
    }

    // ── W-H-04: MR review surface ─────────────────────────────────────

    async fn list_review_threads(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
    ) -> Result<Vec<HostReviewThread>, HostError> {
        <Self as GitlabMerge>::list_review_threads(self, repo, mr_iid).await
    }

    async fn create_review_comment(
        &self,
        input: HostReviewCommentInput<'_>,
    ) -> Result<HostReviewComment, HostError> {
        <Self as GitlabMerge>::create_review_comment(self, input).await
    }

    async fn submit_review(
        &self,
        input: HostSubmitReviewInput<'_>,
    ) -> Result<HostReview, HostError> {
        <Self as GitlabMerge>::submit_review(self, input).await
    }

    // ── W-H-05: MR merge with exact-SHA fence ─────────────────────────

    async fn merge_mr(&self, input: HostMergeInput<'_>) -> Result<HostMergeResult, HostError> {
        <Self as GitlabMerge>::merge_mr(self, input).await
    }

    // ── W-H-06: CI pipelines + jobs (W-P4 dispatch via GitlabBrowse) ──

    async fn list_pipelines(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostPipeline>, HostError> {
        <Self as GitlabBrowse>::list_pipelines(self, repo, ref_name, page).await
    }

    async fn list_jobs(
        &self,
        repo: &RepoRef,
        pipeline_id: &str,
    ) -> Result<Vec<HostJob>, HostError> {
        <Self as GitlabBrowse>::list_jobs(self, repo, pipeline_id).await
    }

    async fn get_job_log(&self, repo: &RepoRef, job_id: &str) -> Result<HostJobLog, HostError> {
        <Self as GitlabBrowse>::get_job_log(self, repo, job_id).await
    }
}

#[cfg(test)]
#[path = "gitlab_tests.rs"]
mod tests;
