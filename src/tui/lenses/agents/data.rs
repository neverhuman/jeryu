//! Owner: Interactive TUI subsystem - Agents lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::agents::data`
//! Invariants: Pure projection from `TuiReadModel` to `AgentsLensInput`. No I/O.
//!             Render layer reads only the resulting struct.

use crate::api::read_model::TuiReadModel;

/// Snapshot of the agent fleet / lifecycle for the Agents lens.
///
/// Projected from `model.mission`: active/blocked agent counts, the number of
/// live grants the fleet is operating under, and whether agents are currently
/// authorized to write code.
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

    /// True when any agent is currently blocked — drives the blocked-row alert
    /// styling and the footer attention hint.
    pub fn has_blocked(&self) -> bool {
        self.blocked_agents > 0
    }

    /// Low-noise fleet status word: a code freeze or any blocked agent is more
    /// urgent than a quiet, healthy fleet.
    pub fn fleet_status(&self) -> &'static str {
        if !self.agents_can_code {
            "FROZEN"
        } else if self.has_blocked() {
            "blocked"
        } else if self.active_agents == 0 {
            "idle"
        } else {
            "active"
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
        assert_eq!(input.blocked_agents, 0);
        assert_eq!(input.active_grants, 0);
        assert_eq!(input.event_cursor, 0);
        // Default mission posture authorizes coding.
        assert!(input.agents_can_code);
        assert!(!input.has_blocked());
        assert_eq!(input.fleet_status(), "idle");
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = AgentsLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn select_projects_agent_fleet_fields() {
        let mut model = TuiReadModel::default();
        model.mission.active_agents = 5;
        model.mission.blocked_agents = 2;
        model.mission.active_grants = 9;
        model.mission.agents_can_code = true;
        let input = AgentsLensInput::from_read_model(&model);
        assert_eq!(input.active_agents, 5);
        assert_eq!(input.blocked_agents, 2);
        assert_eq!(input.active_grants, 9);
        assert!(input.has_blocked());
        // A blocked agent outranks the otherwise-active fleet.
        assert_eq!(input.fleet_status(), "blocked");
    }

    #[test]
    fn fleet_status_flags_code_freeze() {
        let mut model = TuiReadModel::default();
        model.mission.active_agents = 3;
        model.mission.agents_can_code = false;
        let input = AgentsLensInput::from_read_model(&model);
        assert_eq!(input.fleet_status(), "FROZEN");
    }
}
