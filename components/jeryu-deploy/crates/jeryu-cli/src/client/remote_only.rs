//! Production guard for capabilities without an HTTP command adapter.
//!
//! These historical commands had only simulated implementations. Reject them
//! explicitly until a server capability exists; never create temporary state.

use super::*;

/// A client that requires commands to use their live HTTP adapters.
pub struct RemoteOnlyClient;

macro_rules! require_remote {
    ($(fn $method:ident($($arg:ident: $ty:ty),*) -> $result:ty;)*) => {
        impl ForgeClient for RemoteOnlyClient {
            $(fn $method(&self, $($arg: $ty),*) -> ClientResult<$result> {
                $(let _ = $arg;)*
                Err(ClientError::NotWired(concat!(
                    stringify!($method),
                    " has no server transport; no operation was performed"
                ).into()))
            })*
        }
    };
}

require_remote! {
    fn create_repository(owner: &str, req: CreateRepositoryRequest) -> Repository;
    fn list_repositories(owner: Option<&str>) -> Vec<Repository>;
    fn create_issue(owner: &str, repo: &str, req: CreateIssueRequest) -> Issue;
    fn list_issues(owner: &str, repo: &str) -> Vec<Issue>;
    fn open_pull_request(owner: &str, repo: &str, req: OpenPullRequestRequest) -> PullRequest;
    fn list_pull_requests(owner: &str, repo: &str) -> Vec<PullRequest>;
    fn get_pull_request(owner: &str, repo: &str, number: u64) -> PullRequest;
    fn merge_pull_request(owner: &str, repo: &str, number: u64) -> MergeOutcome;
    fn ci_run(repo: &str, git_ref: &str, kind: CiKind) -> CiRun;
    fn ci_status(repo: &str) -> Vec<CiRun>;
    fn ci_explain(run_id: &str) -> CiExplanation;
    fn runner_list() -> Vec<Runner>;
    fn runner_enroll(node: &str, executor: RunnerExecutor) -> Runner;
    fn runner_drain(id: &str) -> Runner;
    fn runner_rotate(id: &str) -> String;
    fn proof_verify(changeset: &str) -> ProofVerdict;
    fn proof_explain(id: &str) -> ProofVerdict;
    fn release_ready(version: &str) -> ReleaseRecord;
    fn cache_self_test() -> CacheSelfTest;
    fn agent_auth_import(tool: AgentTool) -> AgentAuthImportReceipt;
    fn agent_auth_doctor(tool: AgentTool) -> AgentAuthDoctor;
    fn agent_run(request: AgentRunRequest) -> AgentRunStatus;
    fn agent_status(run_id: &str) -> AgentRunStatus;
    fn agent_control(run_id: &str, control: AgentControl) -> AgentRunStatus;
    fn agent_export_pr(request: AgentExportPrRequest) -> AgentExportPr;
}
