//! Owner: Interactive TUI subsystem - Runners lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::runners::data`
//! Invariants: Pure projection from `TuiReadModel` to `RunnersLensInput`.
//!             No I/O.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct RunnersLensInput {
    pub active_runners: u32,
    pub total_runners: u32,
    pub event_cursor: u64,
}

impl RunnersLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            active_runners: model.mission.active_runners,
            total_runners: model.mission.total_runners,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = RunnersLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_runners, 0);
        assert_eq!(input.total_runners, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 99;
        let input = RunnersLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 99);
    }
}
