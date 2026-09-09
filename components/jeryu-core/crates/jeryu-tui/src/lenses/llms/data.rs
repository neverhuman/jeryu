//! Owner: Interactive TUI subsystem - LLMs lens data selector
//! Proof: `cargo test -p jeryu-tui --lib tui::lenses::llms::data`
//! Invariants: Pure projection from `TuiReadModel` to `LlmsLensInput`. No I/O.
//!             The read model carries no dedicated LLM telemetry yet, so this
//!             lens projects the best available proxy for model/agent access:
//!             `mission.active_agents` (the live LLM consumers), `active_grants`
//!             (secret/credential grants gating model access), and
//!             `agents_can_code` (the access posture). Richer per-model
//!             telemetry — provider, model id, token spend, latency, budget,
//!             trace links — arrives once the read model carries a dedicated
//!             `LlmsSnapshot`; this struct gains those fields then without
//!             changing its public surface.

use jeryu_readmodel::TuiReadModel;

#[derive(Debug, Clone, Default)]
pub struct LlmsLensInput {
    /// Active agent sessions — the live consumers of LLM/model access.
    pub active_agents: u32,
    /// Active credential grants gating model/provider access.
    pub active_grants: u32,
    /// Access posture: are agents currently allowed to write code?
    pub agents_can_code: bool,
    pub event_cursor: u64,
}

impl LlmsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            active_agents: model.mission.active_agents,
            active_grants: model.mission.active_grants,
            agents_can_code: model.mission.agents_can_code,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::MissionSnapshot;

    #[test]
    fn select_from_default_read_model_has_zero_cursor() {
        let input = LlmsLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.event_cursor, 0);
        assert_eq!(input.active_agents, 0);
        assert_eq!(input.active_grants, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = LlmsLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn select_projects_mission_access_proxy() {
        let model = TuiReadModel {
            mission: MissionSnapshot {
                active_agents: 5,
                active_grants: 3,
                agents_can_code: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let input = LlmsLensInput::from_read_model(&model);
        assert_eq!(input.active_agents, 5);
        assert_eq!(input.active_grants, 3);
        assert!(!input.agents_can_code);
    }
}
