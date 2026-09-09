//! Forge domain: repository, issue, and pull-request types plus the
//! [`InMemoryClient`] implementation of the forge surface of [`ForgeClient`].

use serde::{Deserialize, Serialize};

use super::{ClientError, ClientResult, InMemoryClient, lock};

// ---------------------------------------------------------------------------
// Forge domain types (GitHub-shaped)
// ---------------------------------------------------------------------------

/// A repository, addressed by `{owner}/{name}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    /// Owning org or user login.
    pub owner: String,
    /// Repository name.
    pub name: String,
    /// Whether the repository is private.
    pub private: bool,
    /// Default branch (e.g. `main`).
    pub default_branch: String,
}

/// A request to create a repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateRepositoryRequest {
    /// Repository name.
    pub name: String,
    /// Whether the repository is private.
    pub private: bool,
    /// Optional default branch override.
    pub default_branch: Option<String>,
}

/// An issue with a per-repo `number`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// Per-repo issue number.
    pub number: u64,
    /// Issue title.
    pub title: String,
    /// Open/closed state.
    pub state: IssueState,
}

/// Issue lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    /// Issue is open.
    Open,
    /// Issue is closed.
    Closed,
}

/// A request to create an issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateIssueRequest {
    /// Issue title.
    pub title: String,
    /// Optional issue body.
    pub body: Option<String>,
}

/// A pull request with a per-repo `number` (GitHub-style, never a per-project IID).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    /// Per-repo pull request number; rendered as `#{number}`.
    pub number: u64,
    /// Source branch.
    pub head: String,
    /// Target branch.
    pub base: String,
    /// Pull request title.
    pub title: String,
    /// Whether the pull request is a draft.
    pub draft: bool,
    /// Lifecycle state.
    pub state: PullRequestState,
}

/// Pull request lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PullRequestState {
    /// Open and not yet merged.
    Open,
    /// Merged into the base branch.
    Merged,
    /// Closed without merge.
    Closed,
}

/// A request to open a pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenPullRequestRequest {
    /// Source branch.
    pub head: String,
    /// Target branch.
    pub base: String,
    /// Pull request title.
    pub title: String,
    /// Whether to open as a draft.
    pub draft: bool,
}

/// Outcome of a merge attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeOutcome {
    /// Pull request number that was merged.
    pub number: u64,
    /// Whether the merge succeeded.
    pub merged: bool,
    /// Human-readable summary.
    pub message: String,
}

// ---------------------------------------------------------------------------
// In-memory implementation: forge surface
// ---------------------------------------------------------------------------

impl InMemoryClient {
    pub(super) fn create_repository_inner(
        &self,
        owner: &str,
        req: CreateRepositoryRequest,
    ) -> ClientResult<Repository> {
        if req.name.trim().is_empty() {
            return Err(ClientError::Invalid("repository name is empty".into()));
        }
        let mut state = lock(&self.state);
        let key = (owner.to_string(), req.name.clone());
        if state.repos.contains_key(&key) {
            return Err(ClientError::Conflict(format!(
                "repository {owner}/{}",
                req.name
            )));
        }
        let repo = Repository {
            owner: owner.to_string(),
            name: req.name,
            private: req.private,
            default_branch: req.default_branch.unwrap_or_else(|| "main".to_string()),
        };
        state.repos.insert(key, repo.clone());
        Ok(repo)
    }

    pub(super) fn list_repositories_inner(
        &self,
        owner: Option<&str>,
    ) -> ClientResult<Vec<Repository>> {
        let state = lock(&self.state);
        Ok(state
            .repos
            .values()
            .filter(|r| owner.is_none_or(|o| r.owner == o))
            .cloned()
            .collect())
    }

    pub(super) fn create_issue_inner(
        &self,
        owner: &str,
        repo: &str,
        req: CreateIssueRequest,
    ) -> ClientResult<Issue> {
        if req.title.trim().is_empty() {
            return Err(ClientError::Invalid("issue title is empty".into()));
        }
        let mut state = lock(&self.state);
        if !state
            .repos
            .contains_key(&(owner.to_string(), repo.to_string()))
        {
            return Err(ClientError::NotFound(format!("repository {owner}/{repo}")));
        }
        let bucket = state
            .issues
            .entry((owner.to_string(), repo.to_string()))
            .or_default();
        let number = bucket.len() as u64 + 1;
        let issue = Issue {
            number,
            title: req.title,
            state: IssueState::Open,
        };
        bucket.push(issue.clone());
        Ok(issue)
    }

    pub(super) fn list_issues_inner(&self, owner: &str, repo: &str) -> ClientResult<Vec<Issue>> {
        let state = lock(&self.state);
        Ok(
            match state.issues.get(&(owner.to_string(), repo.to_string())) {
                Some(issues) => issues.clone(),
                None => Vec::new(),
            },
        )
    }

    pub(super) fn open_pull_request_inner(
        &self,
        owner: &str,
        repo: &str,
        req: OpenPullRequestRequest,
    ) -> ClientResult<PullRequest> {
        if req.title.trim().is_empty() {
            return Err(ClientError::Invalid("pull request title is empty".into()));
        }
        if req.head.trim().is_empty() || req.base.trim().is_empty() {
            return Err(ClientError::Invalid("head and base are required".into()));
        }
        let mut state = lock(&self.state);
        if !state
            .repos
            .contains_key(&(owner.to_string(), repo.to_string()))
        {
            return Err(ClientError::NotFound(format!("repository {owner}/{repo}")));
        }
        let bucket = state
            .pulls
            .entry((owner.to_string(), repo.to_string()))
            .or_default();
        let number = bucket.len() as u64 + 1;
        let pr = PullRequest {
            number,
            head: req.head,
            base: req.base,
            title: req.title,
            draft: req.draft,
            state: PullRequestState::Open,
        };
        bucket.push(pr.clone());
        Ok(pr)
    }

    pub(super) fn list_pull_requests_inner(
        &self,
        owner: &str,
        repo: &str,
    ) -> ClientResult<Vec<PullRequest>> {
        let state = lock(&self.state);
        Ok(
            match state.pulls.get(&(owner.to_string(), repo.to_string())) {
                Some(pulls) => pulls.clone(),
                None => Vec::new(),
            },
        )
    }

    pub(super) fn get_pull_request_inner(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> ClientResult<PullRequest> {
        let state = lock(&self.state);
        match state
            .pulls
            .get(&(owner.to_string(), repo.to_string()))
            .and_then(|b| b.iter().find(|p| p.number == number).cloned())
        {
            Some(pull_request) => Ok(pull_request),
            None => Err(ClientError::NotFound(format!(
                "pull request {owner}/{repo}#{number}"
            ))),
        }
    }

    pub(super) fn merge_pull_request_inner(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> ClientResult<MergeOutcome> {
        let mut state = lock(&self.state);
        let bucket = match state.pulls.get_mut(&(owner.to_string(), repo.to_string())) {
            Some(bucket) => bucket,
            None => return Err(ClientError::NotFound(format!("repository {owner}/{repo}"))),
        };
        let pr = match bucket.iter_mut().find(|p| p.number == number) {
            Some(pr) => pr,
            None => {
                return Err(ClientError::NotFound(format!(
                    "pull request {owner}/{repo}#{number}"
                )));
            }
        };
        if pr.state == PullRequestState::Closed {
            return Err(ClientError::Invalid(format!(
                "pull request #{number} is closed"
            )));
        }
        pr.state = PullRequestState::Merged;
        Ok(MergeOutcome {
            number,
            merged: true,
            message: format!("pull request #{number} merged into {}", pr.base),
        })
    }
}
