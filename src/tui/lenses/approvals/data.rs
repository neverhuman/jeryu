//! Owner: Interactive TUI subsystem - Approvals lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::approvals::data`
//! Invariants: Pure projection from the approvals queue to `ApprovalsLensInput`.
//!             No I/O, no lifetimes — each `PendingApproval` is cloned into an
//!             owned row so the render layer reads only the resulting struct.
//!             The selected index is clamped to the queue so the detail pane
//!             always points at a real row (or none, when the queue is empty).

use crate::tui::app::PendingApproval;

/// One PR awaiting human approval, projected (owned) from a `PendingApproval`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApprovalRow {
    pub pr_number: u64,
    pub title: String,
    pub agent_id: String,
    pub risk_tier: u8,
    pub ci_status: String,
    pub age: String,
    pub head_sha: String,
}

impl ApprovalRow {
    fn from_pending(p: &PendingApproval) -> Self {
        Self {
            pr_number: p.pr_number,
            title: p.title.clone(),
            agent_id: p.agent_id.clone(),
            risk_tier: p.risk_tier,
            ci_status: p.ci_status.clone(),
            age: p.age.clone(),
            head_sha: p.head_sha.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApprovalsLensInput {
    /// Pending MR/PR approvals awaiting a human, in queue order.
    pub rows: Vec<ApprovalRow>,
    /// Index of the highlighted row, clamped to `rows`.
    pub selected: usize,
}

impl ApprovalsLensInput {
    /// Project the live approvals queue into an owned lens input.
    ///
    /// `queue` is the pending-approval list and `selected` is the operator's
    /// highlighted index (`app.selected_approval_index`). The index is clamped
    /// to the last row so the detail pane never points past the queue.
    pub fn from_state(queue: &[PendingApproval], selected: usize) -> Self {
        let rows: Vec<ApprovalRow> = queue.iter().map(ApprovalRow::from_pending).collect();
        let selected = if rows.is_empty() {
            0
        } else {
            selected.min(rows.len() - 1)
        };
        Self { rows, selected }
    }

    /// The currently highlighted row, if the queue is non-empty.
    pub fn selected_row(&self) -> Option<&ApprovalRow> {
        self.rows.get(self.selected)
    }

    /// True when no PRs are awaiting approval.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(pr: u64, tier: u8, ci: &str, title: &str) -> PendingApproval {
        PendingApproval {
            pr_number: pr,
            title: title.into(),
            agent_id: format!("agent-{pr}"),
            risk_tier: tier,
            ci_status: ci.into(),
            age: "3m".into(),
            head_sha: "deadbeefcafef00d".into(),
        }
    }

    #[test]
    fn empty_queue_yields_empty_input_selected_preserved() {
        let input = ApprovalsLensInput::from_state(&[], 0);
        assert!(input.is_empty());
        assert!(input.rows.is_empty());
        assert_eq!(input.selected, 0);
        assert!(input.selected_row().is_none());
    }

    #[test]
    fn selected_index_is_preserved_within_bounds() {
        let queue = vec![
            pending(101, 1, "green", "fix flaky test"),
            pending(102, 3, "running", "bump tier policy"),
            pending(103, 0, "failed", "docs typo"),
        ];
        let input = ApprovalsLensInput::from_state(&queue, 1);
        assert_eq!(input.rows.len(), 3);
        assert_eq!(input.selected, 1);
        let row = input.selected_row().expect("row exists");
        assert_eq!(row.pr_number, 102);
        assert_eq!(row.risk_tier, 3);
        assert_eq!(row.ci_status, "running");
    }

    #[test]
    fn out_of_range_selected_is_clamped_to_last_row() {
        let queue = vec![pending(7, 2, "green", "small change")];
        let input = ApprovalsLensInput::from_state(&queue, 99);
        assert_eq!(input.selected, 0);
        assert_eq!(input.selected_row().unwrap().pr_number, 7);
    }

    #[test]
    fn rows_clone_all_pending_fields() {
        let queue = vec![pending(42, 4, "pass", "risky migration")];
        let input = ApprovalsLensInput::from_state(&queue, 0);
        let row = &input.rows[0];
        assert_eq!(row.pr_number, 42);
        assert_eq!(row.title, "risky migration");
        assert_eq!(row.agent_id, "agent-42");
        assert_eq!(row.risk_tier, 4);
        assert_eq!(row.ci_status, "pass");
        assert_eq!(row.age, "3m");
        assert_eq!(row.head_sha, "deadbeefcafef00d");
    }
}
