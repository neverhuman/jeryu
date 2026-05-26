//! Compatibility shim for the split workflow view.
//!
//! New code should import `crate::tui::lenses::workflow::{view, canvas}`.

pub use crate::tui::lenses::workflow::canvas::{
    draw_dag_canvas, draw_dag_canvas_with_hits, draw_workflow_empty_state,
};
pub use crate::tui::lenses::workflow::view::{
    DeliveryChrome, draw_delivery_tab, draw_delivery_tab_with_chrome, draw_workflow_tab,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_split_view() {
        let _ = DeliveryChrome::default();
        let _ = draw_delivery_tab;
        let _ = draw_workflow_empty_state;
    }
}
