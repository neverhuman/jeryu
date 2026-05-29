//! Owner: Interactive TUI subsystem - Autonomy lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::autonomy::data`
//! Invariants: Pure projection from `TuiReadModel` to
//!             `AutonomyLensInput`. No I/O. The render layer reads only the
//!             resulting struct — never the raw read model.

use crate::api::entity::HealthLevel;
use crate::api::read_model::TuiReadModel;

/// Guardrail / safety-posture projection for the Autonomy lens.
///
/// Captures whether autonomous agents are currently permitted to act and how
/// many guardrails (grants) are live, so the operator can read the autonomy
/// posture at a glance.
#[derive(Debug, Clone)]
pub struct AutonomyLensInput {
    /// Number of live capability grants gating autonomous work.
    pub active_grants: u32,
    /// Master switch: are agents allowed to write code right now?
    pub agents_can_code: bool,
    /// Are the merge/code safety gates satisfied for autonomous coding?
    pub safe_to_code: bool,
    /// Count of agents currently blocked (paused / awaiting a grant).
    pub blocked_agents: u32,
    /// Rolled-up health posture for the autonomy subsystem.
    pub overall: HealthLevel,
    /// Monotonic read-model cursor, surfaced in the footer for traceability.
    pub event_cursor: u64,
}

impl AutonomyLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            active_grants: model.mission.active_grants,
            agents_can_code: model.mission.agents_can_code,
            safe_to_code: model.mission.safe_to_code,
            blocked_agents: model.mission.blocked_agents,
            overall: model.mission.overall,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_from_default_read_model_uses_safe_defaults() {
        let input = AutonomyLensInput::from_read_model(&TuiReadModel::default());
        // Default mission: no grants, agents permitted, gate open, none blocked.
        assert_eq!(input.active_grants, 0);
        assert!(input.agents_can_code);
        assert!(input.safe_to_code);
        assert_eq!(input.blocked_agents, 0);
        assert_eq!(input.overall, HealthLevel::Healthy);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = AutonomyLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn select_projects_guardrail_fields() {
        let mut model = TuiReadModel::default();
        model.mission.active_grants = 5;
        model.mission.agents_can_code = false;
        model.mission.safe_to_code = false;
        model.mission.blocked_agents = 3;
        model.mission.overall = HealthLevel::Critical;
        let input = AutonomyLensInput::from_read_model(&model);
        assert_eq!(input.active_grants, 5);
        assert!(!input.agents_can_code);
        assert!(!input.safe_to_code);
        assert_eq!(input.blocked_agents, 3);
        assert_eq!(input.overall, HealthLevel::Critical);
    }
}
