//! The forge client seam.
//!
//! Every CLI command is backed by a single trait, [`ForgeClient`], so the
//! command layer never reaches for a concrete backend. Today the only
//! implementation is the in-memory [`StubClient`] used by tests and local
//! rehearsals; later the same trait is implemented over the `jeryu-api`
//! HTTP/`jeryu-core` in-process handles without touching the command layer.
//!
//! The vocabulary here is deliberately GitHub-shaped: pull requests carry a
//! per-repo `number` (not a per-project IID), CI work is a `run`, build
//! machines are `runner`s, and gating verdicts are `proof`s.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

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
// CI domain types (compile -> IR -> schedule, never a polled remote pipeline)
// ---------------------------------------------------------------------------

/// CI input dialect. Foreign-forge YAML dialects are intentionally absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiKind {
    /// GitHub Actions workflow YAML.
    GithubActions,
    /// Native jeryu TOML pipeline.
    NativeToml,
}

/// A scheduled CI run, compiled from a workflow file into the jeryu IR.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiRun {
    /// Opaque run identifier.
    pub id: String,
    /// Repository the run belongs to.
    pub repo: String,
    /// Git ref the run was compiled against.
    pub git_ref: String,
    /// Number of jobs in the compiled pipeline IR.
    pub jobs: u32,
    /// Current run status.
    pub status: CiStatus,
}

/// Status of a CI run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiStatus {
    /// Queued, awaiting a runner lease.
    Queued,
    /// Currently executing on a runner.
    Running,
    /// Completed successfully.
    Passed,
    /// Completed with a failure.
    Failed,
}

/// An explanation of why a CI run is (or is not) blocked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiExplanation {
    /// Run identifier explained.
    pub run_id: String,
    /// Whether the run is blocked from merging.
    pub blocked: bool,
    /// Ordered reasons contributing to the verdict.
    pub reasons: Vec<String>,
}

// ---------------------------------------------------------------------------
// Runner domain types (jeryu runners; never foreign-forge runners or pools)
// ---------------------------------------------------------------------------

/// A registered build runner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runner {
    /// Runner identifier.
    pub id: String,
    /// Executor backing the runner.
    pub executor: RunnerExecutor,
    /// Whether the runner currently accepts leases.
    pub accepting: bool,
}

/// Runner executor backend (OCI-first, then native).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunnerExecutor {
    /// OCI container executor.
    Oci,
    /// Native host executor.
    Native,
}

// ---------------------------------------------------------------------------
// Proof domain types
// ---------------------------------------------------------------------------

/// A proof verdict for a changeset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofVerdict {
    /// Whether the changeset is admissible.
    pub admissible: bool,
    /// Stable plan hash; identical changesets verify to identical hashes.
    pub plan_hash: String,
    /// Ordered blocker reasons when not admissible.
    pub blockers: Vec<String>,
}

// ---------------------------------------------------------------------------
// Release / cache domain types
// ---------------------------------------------------------------------------

/// A signed release record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseRecord {
    /// Release version label.
    pub version: String,
    /// Whether the release gate is satisfied.
    pub ready: bool,
    /// Signed witness digest.
    pub witness: String,
}

/// A cache integrity self-test report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheSelfTest {
    /// Number of probes executed.
    pub probes: u32,
    /// Number of false hits detected (must be zero to pass).
    pub false_hits: u32,
    /// Whether the integrity self-test passed.
    pub passed: bool,
}

// ---------------------------------------------------------------------------
// The seam
// ---------------------------------------------------------------------------

/// The single trait every CLI command is backed by.
///
/// Implementations map each method onto a `jeryu-api` route or `jeryu-core`
/// call. The in-memory [`StubClient`] is the only implementation today.
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
}

// ---------------------------------------------------------------------------
// In-memory stub implementation
// ---------------------------------------------------------------------------

#[derive(Default)]
struct StubState {
    repos: BTreeMap<(String, String), Repository>,
    issues: BTreeMap<(String, String), Vec<Issue>>,
    pulls: BTreeMap<(String, String), Vec<PullRequest>>,
    runners: Vec<Runner>,
    runs: Vec<CiRun>,
    run_seq: u64,
}

/// An in-memory [`ForgeClient`] for tests and local rehearsals.
///
/// It is deterministic: issue/PR numbers and run ids are assigned in a stable
/// order so that snapshot and dispatch tests do not flake.
pub struct StubClient {
    state: Mutex<StubState>,
}

impl Default for StubClient {
    fn default() -> Self {
        Self {
            state: Mutex::new(StubState::default()),
        }
    }
}

impl StubClient {
    /// Construct an empty stub client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a stub client preloaded with a single repository, used by
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

fn lock(state: &Mutex<StubState>) -> std::sync::MutexGuard<'_, StubState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl ForgeClient for StubClient {
    fn create_repository(
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

    fn list_repositories(&self, owner: Option<&str>) -> ClientResult<Vec<Repository>> {
        let state = lock(&self.state);
        Ok(state
            .repos
            .values()
            .filter(|r| owner.is_none_or(|o| r.owner == o))
            .cloned()
            .collect())
    }

    fn create_issue(
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

    fn list_issues(&self, owner: &str, repo: &str) -> ClientResult<Vec<Issue>> {
        let state = lock(&self.state);
        Ok(state
            .issues
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    fn open_pull_request(
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

    fn list_pull_requests(&self, owner: &str, repo: &str) -> ClientResult<Vec<PullRequest>> {
        let state = lock(&self.state);
        Ok(state
            .pulls
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> ClientResult<PullRequest> {
        let state = lock(&self.state);
        state
            .pulls
            .get(&(owner.to_string(), repo.to_string()))
            .and_then(|b| b.iter().find(|p| p.number == number).cloned())
            .ok_or_else(|| ClientError::NotFound(format!("pull request {owner}/{repo}#{number}")))
    }

    fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> ClientResult<MergeOutcome> {
        let mut state = lock(&self.state);
        let bucket = state
            .pulls
            .get_mut(&(owner.to_string(), repo.to_string()))
            .ok_or_else(|| ClientError::NotFound(format!("repository {owner}/{repo}")))?;
        let pr = bucket
            .iter_mut()
            .find(|p| p.number == number)
            .ok_or_else(|| {
                ClientError::NotFound(format!("pull request {owner}/{repo}#{number}"))
            })?;
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

    fn ci_run(&self, repo: &str, git_ref: &str, kind: CiKind) -> ClientResult<CiRun> {
        let mut state = lock(&self.state);
        state.run_seq += 1;
        // The job count is derived deterministically from the input dialect so
        // tests can assert the compile-to-IR path was taken per kind.
        let jobs = match kind {
            CiKind::GithubActions => 2,
            CiKind::NativeToml => 3,
        };
        let run = CiRun {
            id: format!("run-{}", state.run_seq),
            repo: repo.to_string(),
            git_ref: git_ref.to_string(),
            jobs,
            status: CiStatus::Queued,
        };
        state.runs.push(run.clone());
        Ok(run)
    }

    fn ci_status(&self, repo: &str) -> ClientResult<Vec<CiRun>> {
        let state = lock(&self.state);
        Ok(state
            .runs
            .iter()
            .filter(|r| r.repo == repo)
            .cloned()
            .collect())
    }

    fn ci_explain(&self, run_id: &str) -> ClientResult<CiExplanation> {
        let state = lock(&self.state);
        let run = state
            .runs
            .iter()
            .find(|r| r.id == run_id)
            .ok_or_else(|| ClientError::NotFound(format!("ci run {run_id}")))?;
        let blocked = matches!(run.status, CiStatus::Failed);
        let reasons = if blocked {
            vec![format!("run {run_id} failed on {}", run.git_ref)]
        } else {
            vec![format!(
                "run {run_id} is {:?} with {} jobs",
                run.status, run.jobs
            )]
        };
        Ok(CiExplanation {
            run_id: run_id.to_string(),
            blocked,
            reasons,
        })
    }

    fn runner_list(&self) -> ClientResult<Vec<Runner>> {
        Ok(lock(&self.state).runners.clone())
    }

    fn runner_enroll(&self, node: &str, executor: RunnerExecutor) -> ClientResult<Runner> {
        if node.trim().is_empty() {
            return Err(ClientError::Invalid("node name is empty".into()));
        }
        let mut state = lock(&self.state);
        if state.runners.iter().any(|r| r.id == node) {
            return Err(ClientError::Conflict(format!(
                "runner {node} already enrolled"
            )));
        }
        let runner = Runner {
            id: node.to_string(),
            executor,
            accepting: true,
        };
        state.runners.push(runner.clone());
        Ok(runner)
    }

    fn runner_drain(&self, id: &str) -> ClientResult<Runner> {
        let mut state = lock(&self.state);
        let runner = state
            .runners
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("runner {id}")))?;
        runner.accepting = false;
        Ok(runner.clone())
    }

    fn runner_rotate(&self, id: &str) -> ClientResult<String> {
        let state = lock(&self.state);
        if !state.runners.iter().any(|r| r.id == id) {
            return Err(ClientError::NotFound(format!("runner {id}")));
        }
        // Credential issuance lives in jeryu-runnerd's registry; the stub mints
        // a deterministic placeholder so the dispatch path is exercisable.
        Ok(format!("cred-{id}-rotated"))
    }

    fn proof_verify(&self, changeset: &str) -> ClientResult<ProofVerdict> {
        if changeset.trim().is_empty() {
            return Err(ClientError::Invalid("changeset is empty".into()));
        }
        // Deterministic, content-derived hash: identical changesets verify to
        // an identical plan hash (replay-stable contract).
        let admissible = !changeset.contains("FORBIDDEN");
        let plan_hash = stable_hash(changeset);
        let blockers = if admissible {
            Vec::new()
        } else {
            vec!["changeset touches a forbidden path".to_string()]
        };
        Ok(ProofVerdict {
            admissible,
            plan_hash,
            blockers,
        })
    }

    fn proof_explain(&self, id: &str) -> ClientResult<ProofVerdict> {
        if id.trim().is_empty() {
            return Err(ClientError::Invalid("blocker id is empty".into()));
        }
        Ok(ProofVerdict {
            admissible: false,
            plan_hash: stable_hash(id),
            blockers: vec![format!("blocker {id}: gate not satisfied")],
        })
    }

    fn release_ready(&self, version: &str) -> ClientResult<ReleaseRecord> {
        if version.trim().is_empty() {
            return Err(ClientError::Invalid("version is empty".into()));
        }
        Ok(ReleaseRecord {
            version: version.to_string(),
            ready: true,
            witness: stable_hash(version),
        })
    }

    fn cache_self_test(&self) -> ClientResult<CacheSelfTest> {
        Ok(CacheSelfTest {
            probes: 8,
            false_hits: 0,
            passed: true,
        })
    }
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
