//! Delivery status aggregation and phase derivation.

use crate::tui::lenses::workflow::{
    delivery::inputs::{PrInput, TestSpec},
    model::{CanonicalPhase, PrStatus, WorkflowSnapshot, WorkflowStatus},
};

pub(super) fn aggregate_status(tests: &[TestSpec]) -> WorkflowStatus {
    if tests.is_empty() {
        return WorkflowStatus::Waiting;
    }
    if tests
        .iter()
        .any(|test| test.status == WorkflowStatus::Error)
    {
        return WorkflowStatus::Error;
    }
    if tests
        .iter()
        .any(|test| test.status == WorkflowStatus::Running)
    {
        return WorkflowStatus::Running;
    }
    if tests
        .iter()
        .any(|test| test.status == WorkflowStatus::Blocked)
    {
        return WorkflowStatus::Blocked;
    }
    if tests.iter().all(|test| test.status.is_terminal()) {
        WorkflowStatus::Ran
    } else {
        WorkflowStatus::Waiting
    }
}

pub(super) fn derive_furthest_phase(snapshot: &WorkflowSnapshot) -> CanonicalPhase {
    let mut furthest = CanonicalPhase::PreMergeCI;
    for phase in CanonicalPhase::ALL {
        let nodes: Vec<_> = snapshot
            .nodes
            .iter()
            .filter(|node| node.tags.iter().any(|tag| tag == phase.slug()))
            .collect();
        if nodes.is_empty() {
            continue;
        }
        let any_active = nodes
            .iter()
            .any(|node| matches!(node.status, WorkflowStatus::Running));
        let any_blocked = nodes
            .iter()
            .any(|node| matches!(node.status, WorkflowStatus::Blocked | WorkflowStatus::Error));
        let all_terminal = nodes.iter().all(|node| node.status.is_terminal());
        if any_active || any_blocked {
            return phase;
        }
        if all_terminal {
            furthest = phase;
        }
    }
    furthest
}

pub(super) fn derive_pr_status(pr: &PrInput, snapshot: &WorkflowSnapshot) -> PrStatus {
    if pr.draft {
        return PrStatus::Draft;
    }
    if snapshot
        .nodes
        .iter()
        .any(|node| matches!(node.status, WorkflowStatus::Error | WorkflowStatus::Blocked))
    {
        return PrStatus::Blocked;
    }
    if pr.merged_into_main {
        return PrStatus::Merged;
    }
    if snapshot
        .nodes
        .iter()
        .any(|node| matches!(node.status, WorkflowStatus::Running))
    {
        return PrStatus::Running;
    }
    PrStatus::Open
}

pub(super) fn pick_current_node(snapshot: &WorkflowSnapshot) -> Option<String> {
    if let Some(node) = snapshot
        .nodes
        .iter()
        .find(|node| matches!(node.status, WorkflowStatus::Error | WorkflowStatus::Blocked))
    {
        return Some(node.id.clone());
    }
    if let Some(node) = snapshot
        .nodes
        .iter()
        .find(|node| matches!(node.status, WorkflowStatus::Running))
    {
        return Some(node.id.clone());
    }
    snapshot
        .nodes
        .iter()
        .find(|node| matches!(node.status, WorkflowStatus::Waiting))
        .map(|node| node.id.clone())
}

pub(super) fn relabel_phases_to_canonical(snapshot: &mut WorkflowSnapshot) {
    for phase in snapshot.phases.iter_mut() {
        if let Some(first_id) = phase.node_ids.first()
            && let Some(node) = snapshot.nodes.iter().find(|node| &node.id == first_id)
            && let Some(slug) = node.tags.first()
            && let Some(canonical) = CanonicalPhase::ALL
                .iter()
                .find(|canonical| canonical.slug() == slug)
        {
            phase.title = canonical.title().to_string();
            phase.id = canonical.slug().to_string();
        }
    }
}
