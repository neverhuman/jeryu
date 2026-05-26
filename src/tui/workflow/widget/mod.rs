//! Owner: Interactive TUI subsystem — workflow DAG widget module root (U20 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::widget`
//! Invariants: Widget is pure rendering; it never mutates workflow state.
//! Layout: split from monolithic `widget.rs` (1063 LOC) per TUI_RESET_PLAN_FINAL.md §3.2.

pub mod canvas;
pub mod hit_map;
pub mod layout;
pub mod render;

pub use canvas::{
    DeliveryChrome, draw_delivery_tab, draw_delivery_tab_with_chrome, draw_workflow_empty_state,
    draw_workflow_tab,
};
pub use hit_map::draw_dag_canvas_with_hits;
pub use render::draw_dag_canvas;

#[cfg(test)]
mod tests {
    use super::super::builder::build_demo_snapshot;
    use super::render::node_color;
    use super::super::model::WorkflowStatus;
    use crate::tui::theme::Theme;

    #[test]
    fn node_color_maps_all_statuses() {
        let theme = Theme::dark();
        // Ensure every status maps without panic.
        for s in &[
            WorkflowStatus::Waiting,
            WorkflowStatus::Running,
            WorkflowStatus::Ran,
            WorkflowStatus::Error,
            WorkflowStatus::Skipped,
            WorkflowStatus::Cached,
            WorkflowStatus::Blocked,
            WorkflowStatus::Unknown,
        ] {
            let _ = node_color(*s, &theme);
        }
    }

    #[test]
    fn demo_snapshot_has_phases() {
        let snap = build_demo_snapshot();
        assert!(!snap.phases.is_empty());
        assert!(!snap.nodes.is_empty());
    }
}
