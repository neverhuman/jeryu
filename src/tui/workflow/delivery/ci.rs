//! Owner: Interactive TUI subsystem — Delivery CI helpers
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure aggregation over `TestSpec` slices; no mutation of inputs.

use super::TestSpec;
use crate::tui::workflow::model::WorkflowStatus;

/// Aggregate child-node statuses into a parent gate's status.
pub(super) fn aggregate_status(tests: &[TestSpec]) -> WorkflowStatus {
    if tests.is_empty() {
        return WorkflowStatus::Waiting;
    }
    if tests.iter().any(|t| t.status == WorkflowStatus::Error) {
        return WorkflowStatus::Error;
    }
    if tests.iter().any(|t| t.status == WorkflowStatus::Running) {
        return WorkflowStatus::Running;
    }
    if tests.iter().any(|t| t.status == WorkflowStatus::Blocked) {
        return WorkflowStatus::Blocked;
    }
    if tests.iter().all(|t| t.status.is_terminal()) {
        WorkflowStatus::Ran
    } else {
        WorkflowStatus::Waiting
    }
}

// ─── TestSpec builders ───────────────────────────────────────────────────

pub(super) fn test(id: &str, command: &str, status: WorkflowStatus) -> TestSpec {
    TestSpec {
        id: id.into(),
        label: id.into(),
        command: command.into(),
        status,
        progress_pct: None,
        eta_secs: None,
        duration_secs: None,
        reason: None,
        critical_path: false,
    }
}

impl TestSpec {
    pub(super) fn done(mut self, secs: f64) -> Self {
        self.duration_secs = Some(secs);
        self
    }
    pub(super) fn at(mut self, pct: u16, eta: u64) -> Self {
        self.progress_pct = Some(pct);
        self.eta_secs = Some(eta);
        self
    }
    pub(super) fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.into());
        self
    }
}
