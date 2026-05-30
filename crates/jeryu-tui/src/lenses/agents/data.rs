//! Agents lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`AgentsLensInput`]. No
//! I/O. Projects the agent fleet from the read model's agents dashboard: per-
//! session rows (status/task/branch/grants) plus the fleet rollup (active/
//! blocked/grants/can-code) from the dashboard summary, falling back to the
//! mission snapshot when the dashboard summary is absent.

use jeryu_readmodel::{AgentItem, AgentStatus, TuiReadModel};

/// One agent session row in the fleet table.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    pub session_id: String,
    pub label: String,
    pub status: AgentStatus,
    pub current_task: Option<String>,
    pub branch: Option<String>,
    pub grants: u32,
}

impl AgentRow {
    fn from_item(item: &AgentItem) -> Self {
        Self {
            session_id: item.session_id.clone(),
            label: item.label.clone(),
            status: item.status,
            current_task: item.current_task.clone(),
            branch: item.branch.clone(),
            grants: item.grants,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AgentsLensInput {
    pub active_agents: u32,
    pub blocked_agents: u32,
    pub active_grants: u32,
    pub agents_can_code: bool,
    pub rows: Vec<AgentRow>,
    pub event_cursor: u64,
}

impl AgentsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.agents.summary.as_ref();
        let rows: Vec<AgentRow> = model.agents.items.iter().map(AgentRow::from_item).collect();
        Self {
            active_agents: summary
                .map(|s| s.active_sessions)
                .unwrap_or(model.mission.active_agents),
            blocked_agents: summary
                .map(|s| s.blocked_sessions)
                .unwrap_or(model.mission.blocked_agents),
            active_grants: summary
                .map(|s| s.active_grants)
                .unwrap_or(model.mission.active_grants),
            agents_can_code: summary
                .map(|s| s.agents_can_code)
                .unwrap_or(model.mission.agents_can_code),
            rows,
            event_cursor: model.event_cursor,
        }
    }

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
    use jeryu_readmodel::sample_read_model;

    #[test]
    fn empty_from_default_read_model() {
        let input = AgentsLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.active_agents, 0);
        assert_eq!(input.blocked_agents, 0);
        assert_eq!(input.active_grants, 0);
        assert!(input.agents_can_code);
        assert!(input.rows.is_empty());
        assert!(!input.has_blocked());
        assert_eq!(input.fleet_status(), "idle");
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn projects_fleet_from_sample() {
        let model = sample_read_model();
        let input = AgentsLensInput::from_read_model(&model);
        assert_eq!(input.active_agents, 1);
        assert_eq!(input.blocked_agents, 1);
        assert_eq!(input.active_grants, 3);
        assert!(input.agents_can_code);
        assert_eq!(input.rows.len(), 3);
        assert_eq!(input.rows[0].session_id, "agent-wrath-17");
        assert_eq!(input.rows[1].status, AgentStatus::Blocked);
        assert!(input.has_blocked());
        assert_eq!(input.fleet_status(), "blocked");
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn falls_back_to_mission_when_no_summary() {
        let mut model = TuiReadModel::default();
        model.mission.active_agents = 5;
        model.mission.blocked_agents = 2;
        model.mission.active_grants = 9;
        model.mission.agents_can_code = false;
        let input = AgentsLensInput::from_read_model(&model);
        assert_eq!(input.active_agents, 5);
        assert_eq!(input.blocked_agents, 2);
        assert_eq!(input.active_grants, 9);
        assert_eq!(input.fleet_status(), "FROZEN");
    }
}
