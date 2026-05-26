//! Owner: Interactive TUI subsystem - Autonomy lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::autonomy::data`
//! Invariants: Pure projection from `TuiReadModel` to
//!             `AutonomyLensInput`. No I/O.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct AutonomyLensInput {
    pub active_grants: u32,
    pub event_cursor: u64,
}

impl AutonomyLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            active_grants: model.mission.active_grants,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = AutonomyLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_grants, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 123,
            ..Default::default()
        };
        let input = AutonomyLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 123);
    }
}
