//! Owner: Interactive TUI subsystem — Inspector pane unit tests
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Pure logic tests on `InspectorTab` cycling/visibility.

use super::tabs::InspectorTab;
use crate::tui::workflow::model::{AgentStage, WorkflowNode, WorkflowNodeKind};

#[test]
fn tab_cycles_next_and_prev_without_agent() {
    // With no node, `next()` and `prev()` skip the Agent tab. Visible
    // count is ALL.len() - 1 = 5; cycling that many lands back at start.
    let visible = InspectorTab::ALL
        .iter()
        .filter(|t| t.visible_for(None))
        .count();
    assert_eq!(visible, 5);

    let mut t = InspectorTab::Overview;
    for _ in 0..visible {
        t = t.next();
    }
    assert_eq!(t, InspectorTab::Overview);

    let mut t = InspectorTab::Logs;
    t = t.prev();
    assert_eq!(t, InspectorTab::Overview);
}

#[test]
fn agent_tab_visible_only_for_agent_review_nodes() {
    let agent_node = WorkflowNode {
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PreMerge,
        },
        ..Default::default()
    };
    let plain_node = WorkflowNode {
        kind: WorkflowNodeKind::UnitTest,
        ..Default::default()
    };
    assert!(InspectorTab::Agent.visible_for(Some(&agent_node)));
    assert!(!InspectorTab::Agent.visible_for(Some(&plain_node)));
    assert!(!InspectorTab::Agent.visible_for(None));
}

#[test]
fn next_for_includes_agent_on_agent_node() {
    let agent_node = WorkflowNode {
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PreMerge,
        },
        ..Default::default()
    };
    let t = InspectorTab::Overview.next_for(Some(&agent_node));
    assert_eq!(t, InspectorTab::Agent);
}
