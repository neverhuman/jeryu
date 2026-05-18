//! GitLab `GitHost` stub.
//!
//! Codex owns the full GitLab adapter (Phase 4). This stub exists so the
//! `GitHost` trait can be used generically today; calls return
//! `HostError::NotImplemented` until the real adapter lands.

use crate::git_host::{
    CheckRun, CheckRunResult, GitHost, HostError, HostIdentity, MrApproval, PrDiff, PrLiveState,
    PrSummary, RepoRef,
};
use async_trait::async_trait;

pub struct GitLabStubClient {
    pub note: String,
}

impl Default for GitLabStubClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GitLabStubClient {
    pub fn new() -> Self {
        Self {
            note: "GitLab adapter is Phase 4 (Codex owns this)".into(),
        }
    }
}

#[async_trait]
impl GitHost for GitLabStubClient {
    fn id(&self) -> &str {
        "gitlab"
    }
    async fn ping_user(&self) -> Result<HostIdentity, HostError> {
        Err(HostError::NotImplemented)
    }
    async fn post_check_run(&self, _input: CheckRun<'_>) -> Result<CheckRunResult, HostError> {
        Err(HostError::NotImplemented)
    }
    async fn post_mr_comment(
        &self,
        _repo: &RepoRef,
        _mr_iid: &str,
        _body: &str,
    ) -> Result<String, HostError> {
        Err(HostError::NotImplemented)
    }
    async fn approve_mr(&self, _input: MrApproval<'_>) -> Result<String, HostError> {
        Err(HostError::NotImplemented)
    }
    async fn list_open_prs(&self, _repo: &RepoRef) -> Result<Vec<PrSummary>, HostError> {
        Err(HostError::NotImplemented)
    }
    async fn get_pr_state(&self, _repo: &RepoRef, _mr_iid: &str) -> Result<PrLiveState, HostError> {
        Err(HostError::NotImplemented)
    }
    async fn fetch_pr_diff(&self, _repo: &RepoRef, _mr_iid: &str) -> Result<PrDiff, HostError> {
        Err(HostError::NotImplemented)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_not_implemented() {
        let s = GitLabStubClient::new();
        assert_eq!(s.id(), "gitlab");
        assert!(matches!(
            s.ping_user().await,
            Err(HostError::NotImplemented)
        ));
    }

    #[tokio::test]
    async fn gitlab_stub_returns_not_implemented_for_list_open_prs() {
        let s = GitLabStubClient::new();
        let repo = RepoRef::parse("anthropics/claude-code").unwrap();
        assert!(matches!(
            s.list_open_prs(&repo).await,
            Err(HostError::NotImplemented)
        ));
    }

    #[tokio::test]
    async fn gitlab_stub_returns_not_implemented_for_get_pr_state() {
        let s = GitLabStubClient::new();
        let repo = RepoRef::parse("anthropics/claude-code").unwrap();
        assert!(matches!(
            s.get_pr_state(&repo, "1").await,
            Err(HostError::NotImplemented)
        ));
    }

    #[tokio::test]
    async fn gitlab_stub_returns_not_implemented_for_fetch_pr_diff() {
        let s = GitLabStubClient::new();
        let repo = RepoRef::parse("anthropics/claude-code").unwrap();
        assert!(matches!(
            s.fetch_pr_diff(&repo, "1").await,
            Err(HostError::NotImplemented)
        ));
    }
}
