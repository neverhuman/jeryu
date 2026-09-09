//! Approvals lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`ApprovalsLensInput`].
//! No I/O. Each pending PR is cloned into an owned row so the render layer reads
//! only the resulting struct. The selected index is clamped to the queue so the
//! inspector always points at a real row (or none, when empty). GitHub PR shape:
//! PR `number` and CI `checks` status.

use jeryu_readmodel::{ApprovalItem, CheckStatus, RiskTier, TuiReadModel};

/// One PR awaiting human approval, projected (owned) from an [`ApprovalItem`].
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalRow {
    pub pr_number: u64,
    pub title: String,
    pub author: String,
    pub risk: RiskTier,
    pub checks: CheckStatus,
    pub age: String,
    pub head_sha: String,
}

impl ApprovalRow {
    fn from_item(item: &ApprovalItem) -> Self {
        Self {
            pr_number: item.pr_number,
            title: item.title.clone(),
            author: item.author.clone(),
            risk: item.risk,
            checks: item.checks,
            age: item.age.clone(),
            head_sha: item.head_sha.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApprovalsLensInput {
    /// Pending PR approvals awaiting a human, in queue order.
    pub rows: Vec<ApprovalRow>,
    /// Index of the highlighted row, clamped to `rows`.
    pub selected: usize,
    pub event_cursor: u64,
}

impl ApprovalsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self::from_read_model_selecting(model, 0)
    }

    /// Project with an explicit operator-highlighted index (clamped to the
    /// last row so the inspector never points past the queue).
    pub fn from_read_model_selecting(model: &TuiReadModel, selected: usize) -> Self {
        let rows: Vec<ApprovalRow> = model
            .approvals
            .items
            .iter()
            .map(ApprovalRow::from_item)
            .collect();
        let selected = if rows.is_empty() {
            0
        } else {
            selected.min(rows.len() - 1)
        };
        Self {
            rows,
            selected,
            event_cursor: model.event_cursor,
        }
    }

    pub fn selected_row(&self) -> Option<&ApprovalRow> {
        self.rows.get(self.selected)
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Count of pending PRs whose checks are red — drives the queue alert.
    pub fn failing_checks(&self) -> usize {
        self.rows.iter().filter(|r| r.checks.is_failing()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::sample_read_model;

    #[test]
    fn empty_queue_from_default_read_model() {
        let input = ApprovalsLensInput::from_read_model(&TuiReadModel::default());
        assert!(input.is_empty());
        assert_eq!(input.selected, 0);
        assert!(input.selected_row().is_none());
        assert_eq!(input.failing_checks(), 0);
    }

    #[test]
    fn projects_pending_prs_from_sample() {
        let model = sample_read_model();
        let input = ApprovalsLensInput::from_read_model(&model);
        assert_eq!(input.rows.len(), 2);
        assert_eq!(input.rows[0].pr_number, 101);
        assert_eq!(input.rows[0].risk, RiskTier::R2);
        assert_eq!(input.rows[0].checks, CheckStatus::Success);
        assert_eq!(input.rows[1].pr_number, 102);
        assert_eq!(input.rows[1].checks, CheckStatus::Failure);
        assert_eq!(input.failing_checks(), 1);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn selected_index_is_clamped_to_last_row() {
        let model = sample_read_model();
        let input = ApprovalsLensInput::from_read_model_selecting(&model, 99);
        assert_eq!(input.selected, 1);
        assert_eq!(input.selected_row().unwrap().pr_number, 102);
    }

    #[test]
    fn selected_index_preserved_in_range() {
        let model = sample_read_model();
        let input = ApprovalsLensInput::from_read_model_selecting(&model, 1);
        assert_eq!(input.selected, 1);
        assert_eq!(input.selected_row().unwrap().pr_number, 102);
    }
}
