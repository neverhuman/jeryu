//! Structured workcell failure implementations and deterministic repair guidance.

use super::*;

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
    "discard outdated heartbeats or releases that carry a prior epoch",
];

const MERGE_DELETE_FIXES: &[&str] = &[
    "keep merge control in the existing review and queue path",
    "do not route delete requests through the workcell control plane",
];

const REPAIR_STATE_FIXES: &[&str] = &[
    "hold the failed tree before live repair",
    "start live repair only after the workcell is in the held state",
];

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

    pub(super) fn claim_denied(message: impl Into<String>) -> Self {
        Self::new(
            "claim a ready workcell",
            "workcell_claim_denied",
            CLAIM_FIXES,
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub(super) fn startup_rebase_failed(message: impl Into<String>) -> Self {
        Self::new(
            "fetch main and rebase the workcell",
            "workcell_startup_rebase_failed",
            STARTUP_REBASE_FIXES,
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub(super) fn branch_budget_denied(message: impl Into<String>) -> Self {
        Self::new(
            "enforce the agent branch budget",
            "workcell_branch_budget_denied",
            BRANCH_BUDGET_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub(super) fn tar_path_denied(message: impl Into<String>) -> Self {
        Self::new(
            "validate quarantine-first tar paths",
            "workcell_tar_path_denied",
            TAR_PATH_FIXES,
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub(super) fn epoch_fenced(message: impl Into<String>) -> Self {
        Self::new(
            "fence outdated workcell epochs",
            "workcell_epoch_fenced",
            EPOCH_FENCE_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub(super) fn merge_denied(message: impl Into<String>) -> Self {
        Self::new(
            "keep merge control out of the workcell",
            "workcell_merge_denied",
            MERGE_DELETE_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub(super) fn delete_denied(message: impl Into<String>) -> Self {
        Self::new(
            "keep delete control out of the workcell",
            "workcell_delete_denied",
            MERGE_DELETE_FIXES,
            "docs/boundaries.md#workcells",
            "rerun cargo test -p jeryu-runnerd workcell --jobs 40",
            message,
        )
    }

    pub(super) fn repair_state_denied(message: impl Into<String>) -> Self {
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
