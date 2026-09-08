//! Transport-neutral read services for forge consumers.
//!
//! These contracts let an HTTP server, CLI, browser backend, or test fake read
//! forge state without depending on [`ForgeCore`]'s concrete persistence and
//! locking implementation. They intentionally expose domain models and
//! [`ForgeError`](crate::ForgeError), never HTTP status codes or wire types.

use crate::{
    AuditEntry, BranchProtectionRule, CheckRunList, ForgeCore, PullRequest, PullRequestState,
    Repository, Result,
};

/// Repository discovery and metadata reads.
pub trait RepositoryReadService: Send + Sync {
    /// Lists repositories, optionally restricted to one owner.
    fn list_repositories(&self, owner: Option<&str>) -> Result<Vec<Repository>>;

    /// Returns one repository or a typed not-found error.
    fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository>;
}

/// Pull-request list and detail reads.
pub trait PullRequestReadService: Send + Sync {
    /// Lists pull requests, optionally restricted to one lifecycle state.
    fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state: Option<PullRequestState>,
    ) -> Result<Vec<PullRequest>>;

    /// Returns one pull request or a typed not-found error.
    fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> Result<PullRequest>;
}

/// Check-run reads for a repository or one exact commit.
pub trait CheckReadService: Send + Sync {
    /// Lists check runs, optionally restricted to one head commit.
    fn list_check_runs(
        &self,
        owner: &str,
        repo: &str,
        head_sha: Option<&str>,
    ) -> Result<CheckRunList>;
}

/// Branch-protection policy reads.
pub trait BranchProtectionReadService: Send + Sync {
    /// Returns the rule for one branch or a typed not-found error.
    fn get_branch_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<BranchProtectionRule>;
}

/// Append-only audit-trail reads.
pub trait AuditReadService: Send + Sync {
    /// Returns all audit entries for one subject, oldest first.
    fn list_audit(&self, subject: &str) -> Result<Vec<AuditEntry>>;
}

/// Complete transport-neutral forge read surface.
///
/// The blanket implementation lets consumers accept `&dyn ForgeReadService`
/// while small fakes implement only the five focused contracts above.
pub trait ForgeReadService:
    RepositoryReadService
    + PullRequestReadService
    + CheckReadService
    + BranchProtectionReadService
    + AuditReadService
{
}

impl<T> ForgeReadService for T where
    T: RepositoryReadService
        + PullRequestReadService
        + CheckReadService
        + BranchProtectionReadService
        + AuditReadService
        + ?Sized
{
}

impl RepositoryReadService for ForgeCore {
    fn list_repositories(&self, owner: Option<&str>) -> Result<Vec<Repository>> {
        Ok(ForgeCore::list_repositories(self, owner))
    }

    fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        ForgeCore::get_repository(self, owner, repo)
    }
}

impl PullRequestReadService for ForgeCore {
    fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state: Option<PullRequestState>,
    ) -> Result<Vec<PullRequest>> {
        ForgeCore::list_pull_requests(self, owner, repo, state)
    }

    fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> Result<PullRequest> {
        ForgeCore::get_pull_request(self, owner, repo, number)
    }
}

impl CheckReadService for ForgeCore {
    fn list_check_runs(
        &self,
        owner: &str,
        repo: &str,
        head_sha: Option<&str>,
    ) -> Result<CheckRunList> {
        ForgeCore::list_check_runs(self, owner, repo, head_sha)
    }
}

impl BranchProtectionReadService for ForgeCore {
    fn get_branch_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<BranchProtectionRule> {
        ForgeCore::get_branch_protection(self, owner, repo, branch)
    }
}

impl AuditReadService for ForgeCore {
    fn list_audit(&self, subject: &str) -> Result<Vec<AuditEntry>> {
        ForgeCore::list_audit(self, subject)
    }
}
