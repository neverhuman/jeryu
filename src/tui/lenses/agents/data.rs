//! Owner: Interactive TUI subsystem - Agents lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::agents::data`
//! Invariants: Pure projection from `TuiReadModel` to `AgentsLensInput`.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct AgentsLensInput {
    pub active_agents: u32,
    pub blocked_agents: u32,
    pub active_grants: u32,
    pub agents_can_code: bool,
    pub event_cursor: u64,
}

impl AgentsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            active_agents: model.mission.active_agents,
            blocked_agents: model.mission.blocked_agents,
            active_grants: model.mission.active_grants,
            agents_can_code: model.mission.agents_can_code,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_is_zero() {
        let input = AgentsLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_agents, 0);
        assert!(input.agents_can_code);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 77,
            ..Default::default()
        };
        let input = AgentsLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 77);
    }
}
