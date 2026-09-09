//! Workcell control-plane primitives for the shared runner/CI stack.
//!
//! The manager here is intentionally in-memory and narrow: it models a warm
//! pool, epoch-fenced claims, startup rebase enforcement, immutable CI repair
//! snapshots, and quarantine-first tar validation without inventing a new
//! runner class.

mod archive;
mod error;
mod manager;
mod model;

use archive::{is_within_any_root, validate_archive_entry, validate_export_path};

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::{Display, Formatter};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

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

/// One tar archive entry used by the quarantine-first path validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub kind: ArchiveEntryKind,
    pub link_target: Option<PathBuf>,
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

/// Request to hold a failed tree for live repair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldFailedTreeRequest {
    pub claim: WorkcellClaimRequest,
    pub ci_run_id: String,
    pub failed_run_id: String,
    pub failed_receipt_id: String,
    pub failure_log_digest: String,
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

/// In-memory workcell warm-pool and claim ledger.
#[derive(Debug, Default, Clone)]
pub struct WorkcellManager {
    next_id: u64,
    ready_queue: VecDeque<String>,
    cells: BTreeMap<String, WorkcellLease>,
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

#[cfg(test)]
mod tests;
