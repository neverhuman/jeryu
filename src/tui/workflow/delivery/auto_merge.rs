//! Owner: Interactive TUI subsystem — Delivery auto-merge gate
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure status-table function; mirrors the policy
//! "PR auto-merges when pre-merge CI passes and agent review is accepted."

use crate::tui::workflow::model::WorkflowStatus;

pub(super) fn auto_merge_gate_status(
    pre_ci: WorkflowStatus,
    agent_pre: WorkflowStatus,
) -> WorkflowStatus {
    match (pre_ci, agent_pre) {
        (WorkflowStatus::Ran | WorkflowStatus::Cached, WorkflowStatus::Ran) => WorkflowStatus::Ran,
        (WorkflowStatus::Error, _) | (_, WorkflowStatus::Error) | (_, WorkflowStatus::Blocked) => {
            WorkflowStatus::Blocked
        }
        _ => WorkflowStatus::Waiting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_merge_passes_when_all_green() {
        assert_eq!(
            auto_merge_gate_status(WorkflowStatus::Ran, WorkflowStatus::Ran),
            WorkflowStatus::Ran
        );
        assert_eq!(
            auto_merge_gate_status(WorkflowStatus::Error, WorkflowStatus::Ran),
            WorkflowStatus::Blocked
        );
        assert_eq!(
            auto_merge_gate_status(WorkflowStatus::Running, WorkflowStatus::Waiting),
            WorkflowStatus::Waiting
        );
    }
}
