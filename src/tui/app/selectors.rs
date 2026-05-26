//! Owner: Interactive TUI subsystem - pure app selectors
//! Proof: `cargo test -p jeryu --lib tui::app::selectors`
//! Invariants: selectors derive display decisions from immutable state only.

use crate::api::entity::EntityRef;

use super::ActiveTab;
use super::state::{ActionFlowState, AppRoute, FlightDeckState};

pub fn active_tab(state: &FlightDeckState) -> Option<ActiveTab> {
    match state.route {
        AppRoute::Tab(tab) => Some(tab),
        _ => None,
    }
}

pub fn selected_entity(state: &FlightDeckState) -> Option<&EntityRef> {
    state.selected_entity.as_ref()
}

pub fn can_unwind_route(state: &FlightDeckState) -> bool {
    !state.route_history.is_empty()
}

pub fn route_label(state: &FlightDeckState) -> String {
    match &state.route {
        AppRoute::Tab(tab) => format!("tab:{tab:?}").to_ascii_lowercase(),
        AppRoute::Entity(entity) => entity.display(),
        AppRoute::Proof(proof_id) => format!("proof:{proof_id}"),
        AppRoute::Action(action_id) => format!("action:{action_id}"),
    }
}

pub fn pending_action_id(state: &FlightDeckState) -> Option<&str> {
    match &state.action {
        ActionFlowState::Preview { action_id } | ActionFlowState::Executing { action_id, .. } => {
            Some(action_id.as_str())
        }
        ActionFlowState::Idle | ActionFlowState::Receipt { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef};
    use crate::tui::app::reducer::{AppIntent, reduce};

    #[test]
    fn route_label_uses_entity_display() {
        let mut state = FlightDeckState::default();
        reduce(
            &mut state,
            AppIntent::Navigate(AppRoute::Entity(EntityRef::new(EntityKind::Job, "7"))),
        );

        assert_eq!(route_label(&state), "job:7");
        assert!(can_unwind_route(&state));
    }

    #[test]
    fn pending_action_tracks_preview_and_execution_only() {
        let mut state = FlightDeckState::default();
        reduce(
            &mut state,
            AppIntent::BeginActionPreview {
                action_id: "run_tests".into(),
            },
        );
        assert_eq!(pending_action_id(&state), Some("run_tests"));

        reduce(
            &mut state,
            AppIntent::ActionReceipt {
                receipt_id: "receipt-1".into(),
            },
        );
        assert_eq!(pending_action_id(&state), None);
    }
}
