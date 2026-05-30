//! Mission lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`MissionLensInput`].
//! No I/O. The render layer reads only the resulting struct.

use jeryu_readmodel::{AttentionItem, MissionSnapshot, NextActionRecommendation, TuiReadModel};

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
        let model = TuiReadModel {
            event_cursor: 1234,
            ..Default::default()
        };
        let input = MissionLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 1234);
    }

    #[test]
    fn select_projects_attention_and_next_action() {
        let model = jeryu_readmodel::sample_read_model();
        let input = MissionLensInput::from_read_model(&model);
        assert_eq!(input.attention.len(), 1);
        assert!(input.next_action.is_some());
        assert_eq!(input.mission.active_agents, 4);
    }
}
