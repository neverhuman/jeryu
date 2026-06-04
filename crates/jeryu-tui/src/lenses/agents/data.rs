//! Agents lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`AgentsLensInput`]. No
//! I/O. Projects the agent fleet from the read model's agents dashboard, live
//! agent-run driver state, and failed-CI repair workcells.

use jeryu_readmodel::{
    AgentItem, AgentRunIoMode, AgentRunItem, AgentRunStatus, AgentStatus, TuiReadModel,
    WorkcellItem, WorkcellState,
};

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

/// One high-level `/api/v1/agent-runs` row.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunRow {
    pub run_id: String,
    pub label: String,
    pub status: AgentRunStatus,
    pub io_mode: AgentRunIoMode,
    pub workcell_id: Option<String>,
    pub tty_status: Option<String>,
    pub output_bytes_used: u64,
    pub output_bytes_limit: u64,
    pub supported_controls: Vec<String>,
}

impl AgentRunRow {
    fn from_item(item: &AgentRunItem) -> Self {
        Self {
            run_id: item.run_id.clone(),
            label: item.label.clone(),
            status: item.status,
            io_mode: item.io_mode,
            workcell_id: item.workcell_id.clone(),
            tty_status: item.tty_status.clone(),
            output_bytes_used: item.output_bytes_used,
            output_bytes_limit: item.output_bytes_limit,
            supported_controls: item.supported_controls.clone(),
        }
    }

    pub fn output_budget(&self) -> String {
        if self.output_bytes_limit == 0 {
            self.output_bytes_used.to_string()
        } else {
            format!("{}/{}", self.output_bytes_used, self.output_bytes_limit)
        }
    }

    pub fn controls_label(&self) -> String {
        if self.supported_controls.is_empty() {
            "closed".into()
        } else {
            self.supported_controls.join(",")
        }
    }
}

/// Failed-CI workcell row shown next to live runs.
#[derive(Debug, Clone, PartialEq)]
pub struct RepairCellRow {
    pub cell_id: String,
    pub state: WorkcellState,
    pub agent_id: String,
    pub failed_run_id: Option<String>,
    pub failed_receipt_id: Option<String>,
    pub failure_log_digest: Option<String>,
    pub repair_state: Option<String>,
    pub export_state: Option<String>,
}

impl RepairCellRow {
    fn from_item(item: &WorkcellItem) -> Self {
        Self {
            cell_id: item.cell_id.clone(),
            state: item.claim_state,
            agent_id: item.agent_id.clone(),
            failed_run_id: item.failed_run_id.clone(),
            failed_receipt_id: item.failed_receipt_id.clone(),
            failure_log_digest: item.failure_log_digest.clone(),
            repair_state: item.repair_state.clone(),
            export_state: item.export_state.clone(),
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
    pub run_rows: Vec<AgentRunRow>,
    pub repair_cells: Vec<RepairCellRow>,
    pub running_runs: u32,
    pub live_tty_runs: u32,
    pub event_cursor: u64,
}

impl AgentsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.agents.summary.as_ref();
        let rows: Vec<AgentRow> = model.agents.items.iter().map(AgentRow::from_item).collect();
        let run_summary = model.agent_runs.summary.as_ref();
        let run_rows: Vec<AgentRunRow> = model
            .agent_runs
            .items
            .iter()
            .map(AgentRunRow::from_item)
            .collect();
        let repair_cells: Vec<RepairCellRow> = model
            .workcells
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item.claim_state,
                    WorkcellState::Held | WorkcellState::Repairing
                ) && (item.failed_run_id.is_some() || item.repair_state.is_some())
            })
            .map(RepairCellRow::from_item)
            .collect();
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
            running_runs: run_summary
                .map(|s| s.running_runs)
                .unwrap_or_else(|| model.agent_runs.running()),
            live_tty_runs: run_summary
                .map(|s| s.live_tty_runs)
                .unwrap_or_else(|| model.agent_runs.live_tty()),
            run_rows,
            repair_cells,
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
        assert!(input.run_rows.is_empty());
        assert!(input.repair_cells.is_empty());
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
        assert_eq!(input.run_rows.len(), 2);
        assert_eq!(input.running_runs, 1);
        assert_eq!(input.live_tty_runs, 1);
        assert_eq!(input.run_rows[0].io_mode, AgentRunIoMode::Pty);
        assert_eq!(input.run_rows[0].tty_status.as_deref(), Some("live tty"));
        assert_eq!(input.repair_cells.len(), 1);
        assert_eq!(input.repair_cells[0].cell_id, "wc-18");
        assert_eq!(
            input.repair_cells[0].failed_run_id.as_deref(),
            Some("ci-18")
        );
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
