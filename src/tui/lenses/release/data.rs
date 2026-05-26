//! Owner: Interactive TUI subsystem - Release lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::release::data`
//! Invariants: Pure projection from `TuiReadModel` to `ReleaseLensInput`.
//!             No I/O.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct ReleaseLensInput {
    pub safe_to_release: bool,
    pub event_cursor: u64,
}

impl ReleaseLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            safe_to_release: model.mission.safe_to_release,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_not_safe() {
        let input = ReleaseLensInput::from_read_model(&TuiReadModel::default());
        assert!(!input.safe_to_release);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 55,
            ..Default::default()
        };
        let input = ReleaseLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 55);
    }
}
