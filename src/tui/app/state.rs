//! Owner: Interactive TUI subsystem - deterministic app state
//! Proof: `cargo test -p jeryu --lib tui::app::state`
//! Invariants: state structs carry serializable route/action intent only; no I/O handles.

use crate::api::entity::EntityRef;

use super::ActiveTab;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlightDeckState {
    pub route: AppRoute,
    pub route_history: Vec<AppRoute>,
    pub selected_entity: Option<EntityRef>,
    pub read_model_cursor: u64,
    pub action: ActionFlowState,
}

impl Default for FlightDeckState {
    fn default() -> Self {
        Self {
            route: AppRoute::Tab(ActiveTab::default()),
            route_history: Vec::new(),
            selected_entity: None,
            read_model_cursor: 0,
            action: ActionFlowState::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppRoute {
    Tab(ActiveTab),
    Entity(EntityRef),
    Proof(String),
    Action(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionFlowState {
    Idle,
    Preview {
        action_id: String,
    },
    Executing {
        action_id: String,
        idempotency_key: String,
    },
    Receipt {
        receipt_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_starts_on_current_app_tab() {
        let state = FlightDeckState::default();
        assert_eq!(state.route, AppRoute::Tab(ActiveTab::default()));
        assert_eq!(state.read_model_cursor, 0);
        assert_eq!(state.action, ActionFlowState::Idle);
    }
}
