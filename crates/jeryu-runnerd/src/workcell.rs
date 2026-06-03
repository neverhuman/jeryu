//! Workcell control-plane primitives for the shared runner/CI stack.
//!
//! The manager here is intentionally in-memory and narrow: it models a warm
//! pool, epoch-fenced claims, startup rebase enforcement, immutable CI repair
//! snapshots, and quarantine-first tar validation without inventing a new
//! runner class.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Display, Formatter};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

const WORKCELL_MAX_BRANCH_BUDGET: u32 = 5;

const CLAIM_FIXES: &[&str] = &[
    "fetch remote main and rebase before handing the cell to the agent",
    "keep the claim epoch attached to every heartbeat, release, and repair call",
];

const STARTUP_REBASE_FIXES: &[&str] = &[
    "refresh the failed snapshot from the frozen evidence copy",
    "rerun the startup rebase instead of bypassing the failure",
];

const BRANCH_BUDGET_FIXES: &[&str] = &[
    "keep branches namespaced to the agent and workcell",
    "raise the budget only through the explicit five-branch override",
];

const TAR_PATH_FIXES: &[&str] = &[
    "extract only into approved repo roots",
    "reject absolute paths, parent traversal, and special files before unpacking",
];

const EPOCH_FENCE_FIXES: &[&str] = &[
    "refresh the workcell lease before retrying the mutation",
    "discard stale heartbeats or releases that carry an old epoch",
];

const MERGE_DELETE_FIXES: &[&str] = &[
    "keep merge control in the existing review and queue path",
    "do not route delete requests through the workcell control plane",
];

const REPAIR_STATE_FIXES: &[&str] = &[
    "hold the failed tree before live repair",
    "start live repair only after the workcell is in the held state",
];

/// Structured repair guidance for workcell failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkcellError {
    pub purpose: &'static str,
    pub reason: &'static str,
    pub common_fixes: &'static [&'static str],
    pub docs_url: &'static str,
    pub repair_hint: &'static str,
    message: String,
}

impl WorkcellError {
    fn new(
        purpose: &'static str,
        reason: &'static str,
        common_fixes: &'static [&'static str],
        docs_url: &'static str,
        repair_hint: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            purpose,
            reason,
            common_fixes,
            docs_url,
            repair_hint,
            message: message.into(),
        }
    }

    fn claim_denied(message: impl Into<String>) -> Self {
        Self::new(
            "claim a ready workcell",
            "workcell_claim_denied",
            CLAIM_FIXES,
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    fn startup_rebase_failed(message: impl Into<String>) -> Self {
        Self::new(
            "fetch main and rebase the workcell",
            "workcell_startup_rebase_failed",
            STARTUP_REBASE_FIXES,
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    fn branch_budget_denied(message: impl Into<String>) -> Self {
        Self::new(
            "enforce the agent branch budget",
            "workcell_branch_budget_denied",
            BRANCH_BUDGET_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    fn tar_path_denied(message: impl Into<String>) -> Self {
        Self::new(
            "validate quarantine-first tar paths",
            "workcell_tar_path_denied",
            TAR_PATH_FIXES,
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    fn epoch_fenced(message: impl Into<String>) -> Self {
        Self::new(
            "fence stale workcell epochs",
            "workcell_epoch_fenced",
            EPOCH_FENCE_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    fn merge_denied(message: impl Into<String>) -> Self {
        Self::new(
            "keep merge control out of the workcell",
            "workcell_merge_denied",
            MERGE_DELETE_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    fn delete_denied(message: impl Into<String>) -> Self {
        Self::new(
            "keep delete control out of the workcell",
            "workcell_delete_denied",
            MERGE_DELETE_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    fn repair_state_denied(message: impl Into<String>) -> Self {
        Self::new(
            "hold a failed workcell before live repair",
            "workcell_repair_state_denied",
            REPAIR_STATE_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for WorkcellError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.reason, self.message)
    }
}

impl std::error::Error for WorkcellError {}

/// Result alias for workcell operations.
pub type WorkcellResult<T> = Result<T, WorkcellError>;

/// Kind of archive entry the workcell import/export helper is allowed to
/// inspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveEntryKind {
    File,
    Directory,
    Symlink,
    Hardlink,
    CharacterDevice,
    BlockDevice,
    Fifo,
    Socket,
}

impl ArchiveEntryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::CharacterDevice => "character-device",
            Self::BlockDevice => "block-device",
            Self::Fifo => "fifo",
            Self::Socket => "socket",
        }
    }
}

/// One tar archive entry used by the quarantine-first path validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub kind: ArchiveEntryKind,
    pub link_target: Option<PathBuf>,
}

impl ArchiveEntry {
    pub fn new(path: impl Into<PathBuf>, kind: ArchiveEntryKind) -> Self {
        Self {
            path: path.into(),
            kind,
            link_target: None,
        }
    }
}

/// Startup sync required before a workcell is handed to the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StartupSync {
    Rebased {
        main_ref: String,
        base_sha: String,
        head_sha: String,
    },
    Failed {
        main_ref: String,
        base_sha: String,
        head_sha: String,
        reason: String,
    },
}

impl StartupSync {
    pub fn is_rebased(&self) -> bool {
        matches!(self, Self::Rebased { .. })
    }
}

/// Branch budget and namespace policy for one workcell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchPolicy {
    pub agent_id: String,
    pub workcell_id: String,
    pub max_branches: u32,
    pub open_branches: BTreeSet<String>,
    pub merge_allowed: bool,
    pub delete_allowed: bool,
}

impl BranchPolicy {
    pub fn new(agent_id: impl Into<String>, workcell_id: impl Into<String>, budget: u32) -> Self {
        Self {
            agent_id: agent_id.into(),
            workcell_id: workcell_id.into(),
            max_branches: budget.clamp(1, WORKCELL_MAX_BRANCH_BUDGET),
            open_branches: BTreeSet::new(),
            merge_allowed: false,
            delete_allowed: false,
        }
    }

    pub fn namespaced_branch(&self, branch: &str) -> String {
        format!(
            "agents/{}/workcells/{}/{}",
            self.agent_id, self.workcell_id, branch
        )
    }

    pub fn open_branch(&mut self, branch: impl Into<String>) -> WorkcellResult<String> {
        if self.open_branches.len() as u32 >= self.max_branches {
            return Err(WorkcellError::branch_budget_denied(format!(
                "agent {} hit workcell {} branch budget {}",
                self.agent_id, self.workcell_id, self.max_branches
            )));
        }
        let namespaced = self.namespaced_branch(&branch.into());
        self.open_branches.insert(namespaced.clone());
        Ok(namespaced)
    }

    pub fn allow_merge(&self) -> WorkcellResult<()> {
        if self.merge_allowed {
            Ok(())
        } else {
            Err(WorkcellError::merge_denied(format!(
                "merge control stays in review/queue for agent {} workcell {}",
                self.agent_id, self.workcell_id
            )))
        }
    }

    pub fn allow_delete(&self) -> WorkcellResult<()> {
        if self.delete_allowed {
            Ok(())
        } else {
            Err(WorkcellError::delete_denied(format!(
                "delete control stays out of agent {} workcell {}",
                self.agent_id, self.workcell_id
            )))
        }
    }
}

/// Immutable frozen snapshot of a failed CI run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenCiSnapshot {
    pub ci_run_id: String,
    pub failed_run_id: String,
    pub failed_receipt_id: String,
    pub workcell_id: String,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub workspace_root: PathBuf,
    pub repo_roots: Vec<PathBuf>,
    pub allowed_paths: Vec<PathBuf>,
    pub git_status_summary: String,
    pub branch_policy: BranchPolicy,
    pub main_ref: String,
    pub base_sha: String,
    pub head_sha: String,
    pub failure_log_digest: String,
    pub snapshot_age_ms: u64,
    pub heartbeat_healthy: bool,
}

impl FrozenCiSnapshot {
    pub fn from_workcell(
        ci_run_id: impl Into<String>,
        failed_run_id: impl Into<String>,
        failed_receipt_id: impl Into<String>,
        workcell: &WorkcellLease,
        failure_log_digest: impl Into<String>,
        snapshot_age_ms: u64,
    ) -> Self {
        let allowed_paths = workcell
            .repo_roots
            .iter()
            .cloned()
            .chain(std::iter::once(workcell.workspace_root.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        Self {
            ci_run_id: ci_run_id.into(),
            failed_run_id: failed_run_id.into(),
            failed_receipt_id: failed_receipt_id.into(),
            workcell_id: workcell.workcell_id.clone(),
            runner_id: workcell.runner_id.clone(),
            runner_epoch: workcell.runner_epoch,
            workspace_root: workcell.workspace_root.clone(),
            repo_roots: workcell.repo_roots.clone(),
            allowed_paths,
            git_status_summary: workcell.git_status_summary.clone(),
            branch_policy: workcell.branch_policy.clone(),
            main_ref: workcell
                .startup_main_ref
                .clone()
                .unwrap_or_else(|| "main".to_string()),
            base_sha: workcell
                .startup_base_sha
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            head_sha: workcell
                .startup_head_sha
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            failure_log_digest: failure_log_digest.into(),
            snapshot_age_ms,
            heartbeat_healthy: workcell.heartbeat_healthy,
        }
    }
}

/// Current lifecycle state of a workcell lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkcellState {
    Warming,
    Ready,
    Claimed,
    Held,
    Repairing,
    Blocked,
    Released,
}

/// Claim request for a workcell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkcellClaimRequest {
    pub agent_id: String,
    pub workspace_root: PathBuf,
    pub repo_roots: Vec<PathBuf>,
    pub branch_budget: u32,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub git_status_summary: String,
    pub ci_snapshot_age_ms: Option<u64>,
    pub startup: StartupSync,
}

/// Request to freeze failed CI evidence before starting live repair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreezeFailedCiRunRequest {
    pub ci_run_id: String,
    pub failed_run_id: String,
    pub failed_receipt_id: String,
    pub failure_log_digest: String,
    pub snapshot_age_ms: u64,
}

/// One live workcell lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkcellLease {
    pub workcell_id: String,
    pub state: WorkcellState,
    pub agent_id: String,
    pub workspace_root: PathBuf,
    pub repo_roots: Vec<PathBuf>,
    pub startup_head_sha: Option<String>,
    pub branch_policy: BranchPolicy,
    pub git_status_summary: String,
    pub ci_snapshot_age_ms: Option<u64>,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub heartbeat_healthy: bool,
    pub startup_rebased: bool,
    pub startup_main_ref: Option<String>,
    pub startup_base_sha: Option<String>,
    pub failed_run_id: Option<String>,
    pub failed_receipt_id: Option<String>,
    pub allowed_paths: Vec<PathBuf>,
    pub failure_log_digest: Option<String>,
    pub frozen_snapshot: Option<FrozenCiSnapshot>,
    pub blocked_reason: Option<String>,
}

impl WorkcellLease {
    fn ready(workcell_id: impl Into<String>) -> Self {
        let workcell_id = workcell_id.into();
        Self {
            workcell_id: workcell_id.clone(),
            state: WorkcellState::Ready,
            agent_id: String::new(),
            workspace_root: PathBuf::new(),
            repo_roots: Vec::new(),
            startup_head_sha: None,
            branch_policy: BranchPolicy::new("", workcell_id, 1),
            git_status_summary: String::new(),
            ci_snapshot_age_ms: None,
            runner_id: String::new(),
            runner_epoch: 0,
            heartbeat_healthy: false,
            startup_rebased: false,
            startup_main_ref: None,
            startup_base_sha: None,
            failed_run_id: None,
            failed_receipt_id: None,
            allowed_paths: Vec::new(),
            failure_log_digest: None,
            frozen_snapshot: None,
            blocked_reason: None,
        }
    }

    fn apply_claim(&mut self, request: &WorkcellClaimRequest) {
        self.state = WorkcellState::Claimed;
        self.agent_id = request.agent_id.clone();
        self.workspace_root = request.workspace_root.clone();
        self.repo_roots = request.repo_roots.clone();
        self.branch_policy = BranchPolicy::new(
            request.agent_id.clone(),
            self.workcell_id.clone(),
            request.branch_budget,
        );
        self.git_status_summary = request.git_status_summary.clone();
        self.ci_snapshot_age_ms = request.ci_snapshot_age_ms;
        self.runner_id = request.runner_id.clone();
        self.runner_epoch = request.runner_epoch;
        self.heartbeat_healthy = true;
        self.startup_rebased = false;
        self.startup_main_ref = None;
        self.startup_base_sha = None;
        self.startup_head_sha = None;
        self.failed_run_id = None;
        self.failed_receipt_id = None;
        self.allowed_paths.clear();
        self.failure_log_digest = None;
        self.frozen_snapshot = None;
        self.blocked_reason = None;
    }

    fn apply_startup(&mut self, startup: &StartupSync) {
        match startup {
            StartupSync::Rebased {
                main_ref,
                base_sha,
                head_sha,
            } => {
                self.startup_rebased = true;
                self.startup_main_ref = Some(main_ref.clone());
                self.startup_base_sha = Some(base_sha.clone());
                self.startup_head_sha = Some(head_sha.clone());
            }
            StartupSync::Failed {
                main_ref,
                base_sha,
                head_sha,
                reason,
            } => {
                self.startup_main_ref = Some(main_ref.clone());
                self.startup_base_sha = Some(base_sha.clone());
                self.startup_head_sha = Some(head_sha.clone());
                self.mark_blocked(format!("startup rebase failed: {reason}"));
            }
        }
    }

    fn mark_blocked(&mut self, reason: impl Into<String>) {
        self.state = WorkcellState::Blocked;
        self.blocked_reason = Some(reason.into());
        self.heartbeat_healthy = false;
    }
}

/// In-memory workcell warm-pool and claim ledger.
#[derive(Debug, Default, Clone)]
pub struct WorkcellManager {
    next_id: u64,
    ready_queue: VecDeque<String>,
    cells: BTreeMap<String, WorkcellLease>,
}

impl WorkcellManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_warm_pool(warm_cells: usize) -> Self {
        let mut manager = Self::new();
        for _ in 0..warm_cells {
            manager.spawn_ready_cell();
        }
        manager
    }

    pub fn ready_count(&self) -> usize {
        self.ready_queue.len()
    }

    pub fn workcell(&self, workcell_id: &str) -> Option<&WorkcellLease> {
        self.cells.get(workcell_id)
    }

    pub fn workcells(&self) -> Vec<WorkcellLease> {
        let mut leases: Vec<_> = self.cells.values().cloned().collect();
        leases.sort_by(|a, b| a.workcell_id.cmp(&b.workcell_id));
        leases
    }

    pub fn claim(&mut self, request: WorkcellClaimRequest) -> WorkcellResult<WorkcellLease> {
        let workcell_id = self
            .ready_queue
            .pop_front()
            .unwrap_or_else(|| self.spawn_ready_cell());
        let outcome = {
            let lease = self
                .cells
                .get_mut(&workcell_id)
                .expect("ready queue and lease table stay in sync");
            lease.apply_claim(&request);
            lease.apply_startup(&request.startup);
            match &request.startup {
                StartupSync::Rebased { .. } => Ok(()),
                StartupSync::Failed {
                    main_ref, reason, ..
                } => Err(WorkcellError::startup_rebase_failed(format!(
                    "workcell {} could not rebase onto {}: {}",
                    lease.workcell_id, main_ref, reason
                ))),
            }
        };

        // Keep the warm pool at a steady depth as soon as a warm cell is
        // consumed.
        self.spawn_ready_cell();

        let lease = self
            .cells
            .get(&workcell_id)
            .expect("ready queue and lease table stay in sync")
            .clone();
        match outcome {
            Ok(()) => Ok(lease),
            Err(err) => Err(err),
        }
    }

    pub fn repair_from_snapshot(
        &mut self,
        snapshot: &FrozenCiSnapshot,
        startup: StartupSync,
    ) -> WorkcellResult<WorkcellLease> {
        let request = WorkcellClaimRequest {
            agent_id: snapshot.branch_policy.agent_id.clone(),
            workspace_root: snapshot.workspace_root.clone(),
            repo_roots: snapshot.repo_roots.clone(),
            branch_budget: snapshot.branch_policy.max_branches,
            runner_id: snapshot.runner_id.clone(),
            runner_epoch: snapshot.runner_epoch.saturating_add(1),
            git_status_summary: snapshot.git_status_summary.clone(),
            ci_snapshot_age_ms: Some(snapshot.snapshot_age_ms),
            startup,
        };
        let mut lease = self.claim(request)?;
        let workcell_id = lease.workcell_id.clone();
        let cell = self
            .cells
            .get_mut(&workcell_id)
            .expect("claimed workcell must stay live");
        cell.state = WorkcellState::Held;
        cell.frozen_snapshot = Some(snapshot.clone());
        cell.branch_policy = snapshot.branch_policy.clone();
        cell.git_status_summary = snapshot.git_status_summary.clone();
        cell.ci_snapshot_age_ms = Some(snapshot.snapshot_age_ms);
        cell.heartbeat_healthy = snapshot.heartbeat_healthy;
        cell.failed_run_id = Some(snapshot.failed_run_id.clone());
        cell.failed_receipt_id = Some(snapshot.failed_receipt_id.clone());
        cell.allowed_paths = snapshot.allowed_paths.clone();
        cell.failure_log_digest = Some(snapshot.failure_log_digest.clone());
        lease = cell.clone();
        Ok(lease)
    }

    pub fn hold_failed_tree(
        &mut self,
        request: WorkcellClaimRequest,
        failed_run_id: impl Into<String>,
        failed_receipt_id: impl Into<String>,
        failure_log_digest: impl Into<String>,
    ) -> WorkcellResult<WorkcellLease> {
        let failed_run_id = failed_run_id.into();
        let failed_receipt_id = failed_receipt_id.into();
        let failure_log_digest = failure_log_digest.into();
        let mut lease = self.claim(request)?;
        let workcell_id = lease.workcell_id.clone();
        let cell = self
            .cells
            .get_mut(&workcell_id)
            .expect("claimed workcell must stay live");
        cell.state = WorkcellState::Held;
        cell.failed_run_id = Some(failed_run_id.clone());
        cell.failed_receipt_id = Some(failed_receipt_id.clone());
        cell.failure_log_digest = Some(failure_log_digest.clone());
        cell.allowed_paths = cell
            .repo_roots
            .iter()
            .cloned()
            .chain(std::iter::once(cell.workspace_root.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let snapshot_source = cell.clone();
        cell.frozen_snapshot = Some(FrozenCiSnapshot::from_workcell(
            failed_run_id.clone(),
            failed_run_id,
            failed_receipt_id,
            &snapshot_source,
            failure_log_digest,
            snapshot_source.ci_snapshot_age_ms.unwrap_or_default(),
        ));
        lease = cell.clone();
        Ok(lease)
    }

    pub fn begin_live_repair(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
    ) -> WorkcellResult<WorkcellLease> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        if !matches!(cell.state, WorkcellState::Held) {
            return Err(WorkcellError::repair_state_denied(format!(
                "workcell {workcell_id} must be held before repair can start"
            )));
        }
        cell.state = WorkcellState::Repairing;
        Ok(cell.clone())
    }

    pub fn export_repair_branch(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
        branch_suffix: impl Into<String>,
    ) -> WorkcellResult<String> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        if !matches!(cell.state, WorkcellState::Held | WorkcellState::Repairing) {
            return Err(WorkcellError::repair_state_denied(format!(
                "workcell {workcell_id} must be held or repairing before export"
            )));
        }
        cell.branch_policy.open_branch(branch_suffix)
    }

    pub fn heartbeat(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
        heartbeat_healthy: bool,
    ) -> WorkcellResult<()> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        if !matches!(
            cell.state,
            WorkcellState::Claimed | WorkcellState::Held | WorkcellState::Repairing
        ) {
            return Err(WorkcellError::claim_denied(format!(
                "workcell {workcell_id} is not active"
            )));
        }
        cell.heartbeat_healthy = heartbeat_healthy;
        Ok(())
    }

    pub fn release(&mut self, workcell_id: &str, runner_epoch: u64) -> WorkcellResult<()> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        cell.state = WorkcellState::Released;
        cell.heartbeat_healthy = false;
        Ok(())
    }

    pub fn block(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
        reason: impl Into<String>,
    ) -> WorkcellResult<()> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        cell.mark_blocked(reason);
        Ok(())
    }

    pub fn freeze_failed_ci_run(
        &self,
        workcell_id: &str,
        runner_epoch: u64,
        request: FreezeFailedCiRunRequest,
    ) -> WorkcellResult<FrozenCiSnapshot> {
        let cell = self.cells.get(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        Ok(FrozenCiSnapshot::from_workcell(
            request.ci_run_id,
            request.failed_run_id,
            request.failed_receipt_id,
            cell,
            request.failure_log_digest,
            request.snapshot_age_ms,
        ))
    }

    fn spawn_ready_cell(&mut self) -> String {
        self.next_id = self.next_id.saturating_add(1);
        let workcell_id = format!("wc-{:04}", self.next_id);
        let lease = WorkcellLease::ready(workcell_id.clone());
        self.ready_queue.push_back(workcell_id.clone());
        self.cells.insert(workcell_id.clone(), lease);
        workcell_id
    }

    fn require_epoch(cell: &WorkcellLease, runner_epoch: u64) -> WorkcellResult<()> {
        if cell.runner_epoch != runner_epoch {
            return Err(WorkcellError::epoch_fenced(format!(
                "workcell {} fenced: epoch {} != active {}",
                cell.workcell_id, runner_epoch, cell.runner_epoch
            )));
        }
        Ok(())
    }
}

/// Validate incoming archive entries before extraction into a workcell repo.
pub fn validate_import_archive(
    entries: &[ArchiveEntry],
    destination_root: impl AsRef<Path>,
    allowed_repo_roots: &[PathBuf],
) -> WorkcellResult<()> {
    let destination_root = destination_root.as_ref();
    if !is_within_any_root(destination_root, allowed_repo_roots) {
        return Err(WorkcellError::tar_path_denied(format!(
            "destination root {} is outside the approved repo roots",
            destination_root.display()
        )));
    }
    for entry in entries {
        validate_archive_entry(entry, destination_root, allowed_repo_roots)?;
    }
    Ok(())
}

/// Validate a set of filesystem paths before exporting them into an outbound
/// tarball.
pub fn validate_export_paths(
    paths: &[PathBuf],
    allowed_repo_roots: &[PathBuf],
) -> WorkcellResult<()> {
    for path in paths {
        validate_export_path(path)?;
        if !is_within_any_root(path, allowed_repo_roots) {
            return Err(WorkcellError::tar_path_denied(format!(
                "export path {} is outside the approved repo roots",
                path.display()
            )));
        }
    }
    Ok(())
}

fn validate_archive_entry(
    entry: &ArchiveEntry,
    destination_root: &Path,
    allowed_repo_roots: &[PathBuf],
) -> WorkcellResult<()> {
    validate_repository_path(&entry.path)?;
    match entry.kind {
        ArchiveEntryKind::File | ArchiveEntryKind::Directory => {}
        _ => {
            return Err(WorkcellError::tar_path_denied(format!(
                "archive entry {} is a {} and cannot be unpacked",
                entry.path.display(),
                entry.kind.as_str()
            )));
        }
    }

    let extracted_path = destination_root.join(&entry.path);
    if !is_within_any_root(&extracted_path, allowed_repo_roots) {
        return Err(WorkcellError::tar_path_denied(format!(
            "archive entry {} extracts outside the approved repo roots",
            entry.path.display()
        )));
    }
    Ok(())
}

fn validate_repository_path(path: &Path) -> WorkcellResult<()> {
    if path.is_absolute() {
        return Err(WorkcellError::tar_path_denied(format!(
            "absolute path {} is not allowed",
            path.display()
        )));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(WorkcellError::tar_path_denied(format!(
                    "path {} contains a forbidden component",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn validate_export_path(path: &Path) -> WorkcellResult<()> {
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::RootDir | Component::Prefix(_) => {}
            _ => {
                return Err(WorkcellError::tar_path_denied(format!(
                    "path {} contains a forbidden component",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn is_within_any_root(path: &Path, allowed_roots: &[PathBuf]) -> bool {
    allowed_roots.iter().any(|root| path.starts_with(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/workspace/core/web")
    }

    #[test]
    fn claim_replaces_warm_cell_and_assigns_branch_budget() {
        let mut manager = WorkcellManager::with_warm_pool(1);
        assert_eq!(manager.ready_count(), 1);

        let lease = manager
            .claim(WorkcellClaimRequest {
                agent_id: "agent-wrath-17".into(),
                workspace_root: root(),
                repo_roots: vec![root()],
                branch_budget: 1,
                runner_id: "xbabe0".into(),
                runner_epoch: 7,
                git_status_summary: "clean".into(),
                ci_snapshot_age_ms: Some(0),
                startup: StartupSync::Rebased {
                    main_ref: "origin/main".into(),
                    base_sha: "abc123".into(),
                    head_sha: "def456".into(),
                },
            })
            .expect("claim succeeds");

        assert_eq!(lease.state, WorkcellState::Claimed);
        assert_eq!(lease.branch_policy.max_branches, 1);
        assert_eq!(
            manager.ready_count(),
            1,
            "a replacement warm cell is spawned"
        );
        assert_eq!(
            manager
                .workcell(&lease.workcell_id)
                .unwrap()
                .startup_main_ref
                .as_deref(),
            Some("origin/main")
        );
    }

    #[test]
    fn startup_rebase_failure_blocks_the_cell() {
        let mut manager = WorkcellManager::with_warm_pool(1);
        let err = manager
            .claim(WorkcellClaimRequest {
                agent_id: "agent-storm-04".into(),
                workspace_root: root(),
                repo_roots: vec![root()],
                branch_budget: 5,
                runner_id: "xbabe1".into(),
                runner_epoch: 8,
                git_status_summary: "dirty".into(),
                ci_snapshot_age_ms: Some(42),
                startup: StartupSync::Failed {
                    main_ref: "origin/main".into(),
                    base_sha: "abc123".into(),
                    head_sha: "def456".into(),
                    reason: "rebase conflict".into(),
                },
            })
            .expect_err("rebase failure must block");

        assert_eq!(err.reason, "workcell_startup_rebase_failed");
        assert!(err.repair_hint.contains("workcell"));
    }

    #[test]
    fn heartbeat_fences_stale_epochs_and_release_marks_released() {
        let mut manager = WorkcellManager::with_warm_pool(1);
        let lease = manager
            .claim(WorkcellClaimRequest {
                agent_id: "agent-wrath-17".into(),
                workspace_root: root(),
                repo_roots: vec![root()],
                branch_budget: 1,
                runner_id: "xbabe0".into(),
                runner_epoch: 7,
                git_status_summary: "clean".into(),
                ci_snapshot_age_ms: None,
                startup: StartupSync::Rebased {
                    main_ref: "origin/main".into(),
                    base_sha: "abc123".into(),
                    head_sha: "def456".into(),
                },
            })
            .expect("claim succeeds");

        let fence = manager
            .heartbeat(&lease.workcell_id, lease.runner_epoch + 1, true)
            .expect_err("stale epoch must fence");
        assert_eq!(fence.reason, "workcell_epoch_fenced");

        manager
            .heartbeat(&lease.workcell_id, lease.runner_epoch, true)
            .expect("matching epoch heartbeat succeeds");
        manager
            .release(&lease.workcell_id, lease.runner_epoch)
            .expect("release succeeds");
        assert_eq!(
            manager.workcell(&lease.workcell_id).unwrap().state,
            WorkcellState::Released
        );
    }

    #[test]
    fn frozen_ci_snapshot_is_immutable_and_repair_uses_it() {
        let mut manager = WorkcellManager::with_warm_pool(1);
        let lease = manager
            .claim(WorkcellClaimRequest {
                agent_id: "agent-wrath-17".into(),
                workspace_root: root(),
                repo_roots: vec![root()],
                branch_budget: 1,
                runner_id: "xbabe0".into(),
                runner_epoch: 7,
                git_status_summary: "clean".into(),
                ci_snapshot_age_ms: Some(100),
                startup: StartupSync::Rebased {
                    main_ref: "origin/main".into(),
                    base_sha: "abc123".into(),
                    head_sha: "def456".into(),
                },
            })
            .expect("claim succeeds");

        let frozen = manager
            .freeze_failed_ci_run(
                &lease.workcell_id,
                lease.runner_epoch,
                FreezeFailedCiRunRequest {
                    ci_run_id: "ci-17".into(),
                    failed_run_id: "run-17".into(),
                    failed_receipt_id: "receipt-17".into(),
                    failure_log_digest: "sha256:deadbeef".into(),
                    snapshot_age_ms: 1_200,
                },
            )
            .expect("freeze succeeds");
        let frozen_before = frozen.clone();
        let repair = manager
            .repair_from_snapshot(
                &frozen,
                StartupSync::Rebased {
                    main_ref: "origin/main".into(),
                    base_sha: "def456".into(),
                    head_sha: "fedcba".into(),
                },
            )
            .expect("repair claim succeeds");

        assert_eq!(frozen, frozen_before, "frozen snapshot must stay immutable");
        assert_eq!(repair.state, WorkcellState::Held);
        assert!(repair.frozen_snapshot.is_some());
        assert_eq!(repair.frozen_snapshot.as_ref().unwrap().ci_run_id, "ci-17");

        let repairing = manager
            .begin_live_repair(&repair.workcell_id, repair.runner_epoch)
            .expect("repair may start after hold");
        assert_eq!(repairing.state, WorkcellState::Repairing);
    }

    #[test]
    fn branch_budget_defaults_to_one_and_caps_at_five() {
        let mut one = BranchPolicy::new("agent-a", "wc-1", 0);
        assert_eq!(one.max_branches, 1);
        assert!(one.open_branch("fix-1").is_ok());
        assert!(one.open_branch("fix-2").is_err());

        let mut five = BranchPolicy::new("agent-a", "wc-2", 9);
        assert_eq!(five.max_branches, 5);
        for idx in 0..5 {
            assert!(
                five.open_branch(format!("branch-{idx}")).is_ok(),
                "branch budget should allow branch {idx}"
            );
        }
        assert!(five.open_branch("branch-6").is_err());
    }

    #[test]
    fn merge_and_delete_are_denied() {
        let policy = BranchPolicy::new("agent-a", "wc-3", 1);
        assert_eq!(
            policy.allow_merge().unwrap_err().reason,
            "workcell_merge_denied"
        );
        assert_eq!(
            policy.allow_delete().unwrap_err().reason,
            "workcell_delete_denied"
        );
    }

    #[test]
    fn tar_helpers_reject_traversal_symlink_and_special_files() {
        let allowed_roots = vec![PathBuf::from("/workspace/core/web")];
        let destination = PathBuf::from("/workspace/core/web");

        assert!(
            validate_import_archive(
                &[ArchiveEntry::new("src/lib.rs", ArchiveEntryKind::File)],
                &destination,
                &allowed_roots,
            )
            .is_ok()
        );

        for entry in [
            ArchiveEntry::new("../escape", ArchiveEntryKind::File),
            ArchiveEntry::new("/abs/path", ArchiveEntryKind::File),
            ArchiveEntry::new("src/link", ArchiveEntryKind::Symlink),
            ArchiveEntry::new("src/hard", ArchiveEntryKind::Hardlink),
            ArchiveEntry::new("dev/tty", ArchiveEntryKind::CharacterDevice),
            ArchiveEntry::new("dev/sda", ArchiveEntryKind::BlockDevice),
            ArchiveEntry::new("tmp/fifo", ArchiveEntryKind::Fifo),
            ArchiveEntry::new("tmp/socket", ArchiveEntryKind::Socket),
        ] {
            let err = validate_import_archive(&[entry], &destination, &allowed_roots)
                .expect_err("unsafe archive entry must be denied");
            assert_eq!(err.reason, "workcell_tar_path_denied");
        }

        assert!(
            validate_export_paths(
                &[PathBuf::from("/workspace/core/web/src/lib.rs")],
                &allowed_roots,
            )
            .is_ok()
        );

        let err = validate_export_paths(
            &[PathBuf::from("/workspace/core/api/src/lib.rs")],
            &allowed_roots,
        )
        .expect_err("export outside repo roots must be denied");
        assert_eq!(err.reason, "workcell_tar_path_denied");
    }
}
