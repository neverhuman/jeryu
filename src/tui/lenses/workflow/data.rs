//! Owner: Interactive TUI subsystem - Workflow lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::workflow::data`
//! Invariants: Pure projection from `TuiReadModel` to `WorkflowLensInput`.
//!             No I/O. Render layer reads only the resulting struct.
//!             Real DAG canvas, critical path, and PR/phase rail land in
//!             U19/U20 proper; this first-cut exposes only aggregate
//!             pipeline counts as placeholders.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct WorkflowLensInput {
    /// Placeholder count. U19 proper pulls per-pipeline entries from
    /// `read_model.workflow` once that surface is contracted.
    pub total_pipelines: u32,
    /// Placeholder count. U19 proper derives this from per-pipeline
    /// status once the workflow projection lands.
    pub blocked_count: u32,
    pub event_cursor: u64,
}

impl WorkflowLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            // placeholders; U19 proper populates from model.workflow
            total_pipelines: 0,
            blocked_count: 0,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_returns_default_input() {
        let model = TuiReadModel::default();
        let input = WorkflowLensInput::from_read_model(&model);
        assert_eq!(input.total_pipelines, 0);
        assert_eq!(input.blocked_count, 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 7890;
        let input = WorkflowLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 7890);
    }
}
