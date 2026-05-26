//! Owner: Interactive TUI subsystem - Mission lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::mission::data`
//! Invariants: Pure projection from `TuiReadModel` to `MissionLensInput`.
//!             No I/O. Render layer reads only the resulting struct.

use crate::api::read_model::{
    AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel,
};

#[derive(Debug, Clone)]
pub struct MissionLensInput {
    pub mission: MissionSnapshot,
    pub attention: Vec<AttentionItem>,
    pub next_action: Option<NextActionRecommendation>,
    pub event_cursor: u64,
}

impl MissionLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            mission: model.mission.clone(),
            attention: model.attention.clone(),
            next_action: model.next_action.clone(),
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_returns_default_mission() {
        let model = TuiReadModel::default();
        let input = MissionLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 0);
        assert!(input.attention.is_empty());
        assert!(input.next_action.is_none());
    }

    #[test]
    fn select_preserves_event_cursor() {
        let mut model = TuiReadModel::default();
        model.event_cursor = 1234;
        let input = MissionLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 1234);
    }
}
