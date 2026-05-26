//! Owner: Interactive TUI subsystem - Bugs lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::bugs::data`
//! Invariants: Pure projection from `TuiReadModel` to `BugsLensInput`.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct BugsLensInput {
    pub open_capsules: u32,
    pub event_cursor: u64,
}

impl BugsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            open_capsules: model.mission.open_capsules,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = BugsLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.open_capsules, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 42;
        let input = BugsLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }
}
