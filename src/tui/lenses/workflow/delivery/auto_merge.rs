//! Auto-merge delivery gate helpers.

use crate::tui::lenses::workflow::model::WorkflowStatus;

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
