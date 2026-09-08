//! The forge client seam.
//!
//! Every CLI command is backed by a single trait, [`ForgeClient`], so the
//! command layer never reaches for a concrete backend. The current
//! implementation is the in-memory [`InMemoryClient`] used by tests and local
//! rehearsals; an implementation over the `jeryu-api` HTTP/`jeryu-core`
//! in-process handles can be added behind the same trait without touching the
//! command layer.
//!
//! The vocabulary here is deliberately GitHub-shaped: pull requests carry a
//! per-repo `number` (not a per-project IID), CI work is a `run`, build
//! machines are `runner`s, and gating verdicts are `proof`s.
//!
//! The seam (this module) owns the error type, the trait, and the in-memory
//! store/constructor. The domain types and the `InMemoryClient` method bodies
//! are grouped by domain area into sibling submodules ([`forge`], [`ci`],
//! [`runner`], [`proof`], [`release`], [`cache`]) and re-exported here so every
//! `crate::client::Thing` path resolves unchanged. The single
//! `impl ForgeClient for InMemoryClient` block that wires the trait surface to
//! those per-domain method bodies lives in [`inmemory`].

mod agent;
mod cache;
mod ci;
mod forge;
mod inmemory;
mod proof;
mod release;
mod runner;

pub use agent::{
    AgentAuthDoctor, AgentAuthImportReceipt, AgentControl, AgentExportPr, AgentExportPrRequest,
    AgentRunRequest, AgentRunStatus, AgentTool,
};
pub use cache::CacheSelfTest;
pub use ci::{CiExplanation, CiKind, CiRun, CiStatus};
pub use forge::{
    CreateIssueRequest, CreateRepositoryRequest, Issue, IssueState, MergeOutcome,
    OpenPullRequestRequest, PullRequest, PullRequestState, Repository,
};
pub use proof::ProofVerdict;
pub use release::ReleaseRecord;
pub use runner::{Runner, RunnerExecutor};

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Errors surfaced across the client seam.
#[derive(Debug)]
pub enum ClientError {
    /// The requested entity does not exist.
    NotFound(String),
    /// The request conflicts with existing state.
    Conflict(String),
    /// The request is structurally invalid.
    Invalid(String),
    /// The backing capability is recognized but not yet wired to a real engine.
    NotWired(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NotFound(m) => write!(f, "not found: {m}"),
            ClientError::Conflict(m) => write!(f, "conflict: {m}"),
            ClientError::Invalid(m) => write!(f, "invalid: {m}"),
            ClientError::NotWired(m) => write!(f, "not yet wired: {m}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// Convenience result alias for client operations.
pub type ClientResult<T> = Result<T, ClientError>;

// ---------------------------------------------------------------------------
// The seam
// ---------------------------------------------------------------------------

/// The single trait every CLI command is backed by.
///
/// Implementations map each method onto a `jeryu-api` route or `jeryu-core`
/// call. The in-memory [`InMemoryClient`] is the current implementation.
pub trait ForgeClient {
    // forge repo
    /// Create a repository. Backs `jeryu forge repo create`.
    fn create_repository(
        &self,
        owner: &str,
        req: CreateRepositoryRequest,
    ) -> ClientResult<Repository>;
    /// List repositories, optionally scoped to an owner. Backs `jeryu forge repo list`.
    fn list_repositories(&self, owner: Option<&str>) -> ClientResult<Vec<Repository>>;

    // forge issue
    /// Create an issue. Backs `jeryu forge issue create`.
    fn create_issue(&self, owner: &str, repo: &str, req: CreateIssueRequest)
    -> ClientResult<Issue>;
    /// List issues. Backs `jeryu forge issue list`.
    fn list_issues(&self, owner: &str, repo: &str) -> ClientResult<Vec<Issue>>;

    // forge pr
    /// Open a pull request. Backs `jeryu forge pr open`.
    fn open_pull_request(
        &self,
        owner: &str,
        repo: &str,
        req: OpenPullRequestRequest,
    ) -> ClientResult<PullRequest>;
    /// List pull requests. Backs `jeryu forge pr list`.
    fn list_pull_requests(&self, owner: &str, repo: &str) -> ClientResult<Vec<PullRequest>>;
    /// Show a single pull request by number. Backs `jeryu forge pr status`.
    fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> ClientResult<PullRequest>;
    /// Merge a pull request, subject to the risk gate. Backs `jeryu forge pr merge`.
    fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> ClientResult<MergeOutcome>;

    // ci
    /// Compile a workflow file into IR and schedule it. Backs `jeryu ci run`.
    fn ci_run(&self, repo: &str, git_ref: &str, kind: CiKind) -> ClientResult<CiRun>;
    /// Report scheduled/queued runs for a repo. Backs `jeryu ci status`.
    fn ci_status(&self, repo: &str) -> ClientResult<Vec<CiRun>>;
    /// Explain the blocking state of a run. Backs `jeryu ci explain`.
    fn ci_explain(&self, run_id: &str) -> ClientResult<CiExplanation>;

    // runner
    /// List registered runners. Backs `jeryu runner list`.
    fn runner_list(&self) -> ClientResult<Vec<Runner>>;
    /// Enroll a node as a runner. Backs `jeryu runner enroll`.
    fn runner_enroll(&self, node: &str, executor: RunnerExecutor) -> ClientResult<Runner>;
    /// Drain a runner: stop accepting leases, await in-flight. Backs `jeryu runner drain`.
    fn runner_drain(&self, id: &str) -> ClientResult<Runner>;
    /// Rotate a runner enrollment credential. Backs `jeryu runner rotate`.
    fn runner_rotate(&self, id: &str) -> ClientResult<String>;

    // proof
    /// Verify a changeset. Backs `jeryu proof verify`.
    fn proof_verify(&self, changeset: &str) -> ClientResult<ProofVerdict>;
    /// Explain a proof blocker by id. Backs `jeryu proof explain`.
    fn proof_explain(&self, id: &str) -> ClientResult<ProofVerdict>;

    // release
    /// Compose the release-ready gate for a version. Backs `jeryu release`.
    fn release_ready(&self, version: &str) -> ClientResult<ReleaseRecord>;

    // cache
    /// Run the cache integrity self-test. Backs `jeryu cache self-test`.
    fn cache_self_test(&self) -> ClientResult<CacheSelfTest>;

    // agent
    /// Import portable agent auth. Backs `jeryu agent auth import`.
    fn agent_auth_import(&self, tool: AgentTool) -> ClientResult<AgentAuthImportReceipt>;
    /// Check imported portable agent auth. Backs `jeryu agent auth doctor`.
    fn agent_auth_doctor(&self, tool: AgentTool) -> ClientResult<AgentAuthDoctor>;
    /// Start an agent-edit run. Backs `jeryu agent run`.
    fn agent_run(&self, request: AgentRunRequest) -> ClientResult<AgentRunStatus>;
    /// Read agent-edit run status. Backs `jeryu agent status`.
    fn agent_status(&self, run_id: &str) -> ClientResult<AgentRunStatus>;
    /// Send agent-edit control. Backs `jeryu agent control`.
    fn agent_control(&self, run_id: &str, control: AgentControl) -> ClientResult<AgentRunStatus>;
    /// Export agent-edit run as PR. Backs `jeryu agent export-pr`.
    fn agent_export_pr(&self, request: AgentExportPrRequest) -> ClientResult<AgentExportPr>;
}

// ---------------------------------------------------------------------------
// In-memory implementation
// ---------------------------------------------------------------------------

#[derive(Default)]
struct InMemoryState {
    repos: BTreeMap<(String, String), Repository>,
    issues: BTreeMap<(String, String), Vec<Issue>>,
    pulls: BTreeMap<(String, String), Vec<PullRequest>>,
    runners: Vec<Runner>,
    runs: Vec<CiRun>,
    run_seq: u64,
    agent_auth: BTreeMap<AgentTool, bool>,
    agent_runs: BTreeMap<String, AgentRunStatus>,
}

/// An in-memory [`ForgeClient`] for tests and local rehearsals.
///
/// It is deterministic: issue/PR numbers and run ids are assigned in a stable
/// order so that snapshot and dispatch tests do not flake.
pub struct InMemoryClient {
    state: Mutex<InMemoryState>,
}

impl Default for InMemoryClient {
    fn default() -> Self {
        Self {
            state: Mutex::new(InMemoryState::default()),
        }
    }
}

impl InMemoryClient {
    /// Construct an empty in-memory client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an in-memory client preloaded with a single repository, used by
    /// dispatch smoke tests that exercise issue/PR/CI verbs.
    #[must_use]
    pub fn with_seed_repo(owner: &str, name: &str) -> Self {
        let client = Self::new();
        client
            .create_repository(
                owner,
                CreateRepositoryRequest {
                    name: name.to_string(),
                    private: false,
                    default_branch: None,
                },
            )
            .expect("seed repo");
        client
    }
}

fn lock(state: &Mutex<InMemoryState>) -> std::sync::MutexGuard<'_, InMemoryState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Deterministic non-cryptographic content hash for replay-stable verdicts.
fn stable_hash(input: &str) -> String {
    // FNV-1a 64-bit; stable across runs and platforms.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
