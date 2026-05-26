//! Owner: Interactive TUI subsystem - LLMs lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::llms::data`
//! Invariants: Pure projection from `TuiReadModel` to `LlmsLensInput`.
//!             No LLM fields exist in `MissionSnapshot` yet; stub fields
//!             land in U25 along with the provider/budget projection.
//!             No I/O.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct LlmsLensInput {
    pub event_cursor: u64,
}

impl LlmsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_has_zero_cursor() {
        let input = LlmsLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 999;
        let input = LlmsLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 999);
    }
}
