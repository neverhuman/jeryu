//! The single trait-wiring block for the in-memory client.
//!
//! Every [`ForgeClient`] method delegates to the matching `*_inner` method
//! defined in the per-domain submodule ([`super::forge`], [`super::ci`],
//! [`super::runner`], [`super::proof`], [`super::release`], [`super::cache`]).
//! Rust requires a trait impl to live in one block, so this file holds the
//! whole surface while the actual logic stays grouped by domain.

use super::{
    AgentAuthDoctor, AgentAuthImportReceipt, AgentControl, AgentExportPr, AgentExportPrRequest,
    AgentRunRequest, AgentRunStatus, AgentTool, CacheSelfTest, CiExplanation, CiKind, CiRun,
    ClientResult, CreateIssueRequest, CreateRepositoryRequest, ForgeClient, InMemoryClient, Issue,
    MergeOutcome, OpenPullRequestRequest, ProofVerdict, PullRequest, ReleaseRecord, Repository,
    Runner, RunnerExecutor,
};

impl ForgeClient for InMemoryClient {
    fn create_repository(
        &self,
        owner: &str,
        req: CreateRepositoryRequest,
    ) -> ClientResult<Repository> {
        self.create_repository_inner(owner, req)
    }

    fn list_repositories(&self, owner: Option<&str>) -> ClientResult<Vec<Repository>> {
        self.list_repositories_inner(owner)
    }

    fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        req: CreateIssueRequest,
    ) -> ClientResult<Issue> {
        self.create_issue_inner(owner, repo, req)
    }

    fn list_issues(&self, owner: &str, repo: &str) -> ClientResult<Vec<Issue>> {
        self.list_issues_inner(owner, repo)
    }

    fn open_pull_request(
        &self,
        owner: &str,
        repo: &str,
        req: OpenPullRequestRequest,
    ) -> ClientResult<PullRequest> {
        self.open_pull_request_inner(owner, repo, req)
    }

    fn list_pull_requests(&self, owner: &str, repo: &str) -> ClientResult<Vec<PullRequest>> {
        self.list_pull_requests_inner(owner, repo)
    }

    fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> ClientResult<PullRequest> {
        self.get_pull_request_inner(owner, repo, number)
    }

    fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> ClientResult<MergeOutcome> {
        self.merge_pull_request_inner(owner, repo, number)
    }

    fn ci_run(&self, repo: &str, git_ref: &str, kind: CiKind) -> ClientResult<CiRun> {
        self.ci_run_inner(repo, git_ref, kind)
    }

    fn ci_status(&self, repo: &str) -> ClientResult<Vec<CiRun>> {
        self.ci_status_inner(repo)
    }

    fn ci_explain(&self, run_id: &str) -> ClientResult<CiExplanation> {
        self.ci_explain_inner(run_id)
    }

    fn runner_list(&self) -> ClientResult<Vec<Runner>> {
        self.runner_list_inner()
    }

    fn runner_enroll(&self, node: &str, executor: RunnerExecutor) -> ClientResult<Runner> {
        self.runner_enroll_inner(node, executor)
    }

    fn runner_drain(&self, id: &str) -> ClientResult<Runner> {
        self.runner_drain_inner(id)
    }

    fn runner_rotate(&self, id: &str) -> ClientResult<String> {
        self.runner_rotate_inner(id)
    }

    fn proof_verify(&self, changeset: &str) -> ClientResult<ProofVerdict> {
        self.proof_verify_inner(changeset)
    }

    fn proof_explain(&self, id: &str) -> ClientResult<ProofVerdict> {
        self.proof_explain_inner(id)
    }

    fn release_ready(&self, version: &str) -> ClientResult<ReleaseRecord> {
        self.release_ready_inner(version)
    }

    fn cache_self_test(&self) -> ClientResult<CacheSelfTest> {
        self.cache_self_test_inner()
    }

    fn agent_auth_import(&self, tool: AgentTool) -> ClientResult<AgentAuthImportReceipt> {
        self.agent_auth_import_inner(tool)
    }

    fn agent_auth_doctor(&self, tool: AgentTool) -> ClientResult<AgentAuthDoctor> {
        self.agent_auth_doctor_inner(tool)
    }

    fn agent_run(&self, request: AgentRunRequest) -> ClientResult<AgentRunStatus> {
        self.agent_run_inner(request)
    }

    fn agent_status(&self, run_id: &str) -> ClientResult<AgentRunStatus> {
        self.agent_status_inner(run_id)
    }

    fn agent_control(&self, run_id: &str, control: AgentControl) -> ClientResult<AgentRunStatus> {
        self.agent_control_inner(run_id, control)
    }

    fn agent_export_pr(&self, request: AgentExportPrRequest) -> ClientResult<AgentExportPr> {
        self.agent_export_pr_inner(request)
    }
}
