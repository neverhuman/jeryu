use crate::tui::lenses::workflow::model::{DeliverySnapshot, WorkflowSnapshot};

#[derive(Debug, Clone, Copy)]
pub struct WorkflowLensInput<'a> {
    pub delivery: &'a DeliverySnapshot,
    pub selected_workflow: Option<&'a WorkflowSnapshot>,
}

pub fn select_workflow_lens_input(delivery: &DeliverySnapshot) -> WorkflowLensInput<'_> {
    WorkflowLensInput {
        delivery,
        selected_workflow: delivery.selected().map(|pr| &pr.snapshot),
    }
}

impl<'a> WorkflowLensInput<'a> {
    pub fn has_prs(self) -> bool {
        !self.delivery.pull_requests.is_empty()
    }

    pub fn selected_title(self) -> Option<&'a str> {
        self.delivery.selected().map(|pr| pr.title.as_str())
    }

    pub fn top_blocker(self) -> Option<&'a str> {
        self.delivery.fleet_summary.top_blocker.as_deref()
    }

    pub fn selected_node_count(self) -> usize {
        self.selected_workflow
            .map_or(0, |snapshot| snapshot.nodes.len())
    }
}
