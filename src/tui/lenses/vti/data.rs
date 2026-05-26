//! Owner: Interactive TUI subsystem - VTI lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::vti::data`
//! Invariants: Pure projection from `TuiReadModel` to `VtiLensInput`.
//!             No I/O.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct VtiLensInput {
    pub selector_misses_24h: u32,
    pub event_cursor: u64,
}

impl VtiLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            selector_misses_24h: model.mission.selector_misses_24h,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = VtiLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.selector_misses_24h, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 88;
        let input = VtiLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 88);
    }
}
