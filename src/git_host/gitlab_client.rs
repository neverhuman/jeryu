//! Owner: GitLab host adapter — inherent impl helpers
//! Proof: `cargo test -p jeryu --lib git_host::gitlab`
//! Invariants: constructors + tiny GET/POST helpers. The GitHost trait impl
//! and pure conversion helpers live in sibling files.

use serde::{Deserialize, Serialize};

use crate::git_host::{
    CheckRun, CheckRunResult, CheckStatus, GitHost, HostError, RepoRef,
    VIBEGATE_MERGE_PASSPORT_CHECK_NAME,
};
use crate::gitlab_client::GitlabClient;

use super::GitLabClient;
use super::gitlab_helpers::map_error;
use super::gitlab_types::{GitLabNote, GitLabNoteReq, GitLabProtectedBranch};

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

    pub(super) fn project_ref(repo: &RepoRef) -> String {
        urlencoding::encode(&repo.slug()).into_owned()
    }

    pub(super) async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
    ) -> Result<T, HostError> {
        self.inner
            .api_get_json(self.inner.api_url(path))
            .await
            .map_err(map_error)
    }

    pub(super) async fn post_json<Req: Serialize, Resp: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp, HostError> {
        self.inner
            .api_post_json(self.inner.api_url(path), body)
            .await
            .map_err(map_error)
    }

    pub(super) async fn post_note(
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
        <Self as GitHost>::post_check_run(
            self,
            CheckRun {
                repo,
                head_sha,
                name: VIBEGATE_MERGE_PASSPORT_CHECK_NAME,
                status,
                summary,
                details_url,
                output_text: None,
            },
        )
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
