//! Owner: Interactive TUI subsystem - deterministic app reducers
//! Proof: `cargo test -p jeryu --lib tui::app::reducer`
//! Invariants: reducers mutate in-memory state only and never call external systems.

use crate::api::entity::EntityRef;

use super::state::{ActionFlowState, AppRoute, FlightDeckState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppIntent {
    Navigate(AppRoute),
    PopRoute,
    SelectEntity(Option<EntityRef>),
    ReadModelLoaded {
        event_cursor: u64,
    },
    BeginActionPreview {
        action_id: String,
    },
    BeginActionExecution {
        action_id: String,
        idempotency_key: String,
    },
    ActionReceipt {
        receipt_id: String,
    },
    ClearAction,
}

pub fn reduce(state: &mut FlightDeckState, intent: AppIntent) {
    match intent {
        AppIntent::Navigate(route) => {
            if state.route != route {
                state.route_history.push(state.route.clone());
                state.route = route;
            }
        }
        AppIntent::PopRoute => {
            if let Some(route) = state.route_history.pop() {
                state.route = route;
            }
        }
        AppIntent::SelectEntity(entity) => {
            state.selected_entity = entity;
        }
        AppIntent::ReadModelLoaded { event_cursor } => {
            state.read_model_cursor = state.read_model_cursor.max(event_cursor);
        }
        AppIntent::BeginActionPreview { action_id } => {
            state.action = ActionFlowState::Preview { action_id };
        }
        AppIntent::BeginActionExecution {
            action_id,
            idempotency_key,
        } => {
            state.action = ActionFlowState::Executing {
                action_id,
                idempotency_key,
            };
        }
        AppIntent::ActionReceipt { receipt_id } => {
            state.action = ActionFlowState::Receipt { receipt_id };
        }
        AppIntent::ClearAction => {
            state.action = ActionFlowState::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::EntityKind;
    use crate::tui::app::ActiveTab;

    #[test]
    fn navigate_pushes_and_pop_route_restores() {
        let mut state = FlightDeckState::default();

        reduce(
            &mut state,
            AppIntent::Navigate(AppRoute::Tab(ActiveTab::Jobs)),
        );
        assert_eq!(state.route, AppRoute::Tab(ActiveTab::Jobs));
        assert_eq!(state.route_history.len(), 1);

        reduce(&mut state, AppIntent::PopRoute);
        assert_eq!(state.route, AppRoute::Tab(ActiveTab::default()));
    }

    #[test]
    fn selected_entity_is_id_anchored() {
        let mut state = FlightDeckState::default();
        let entity = EntityRef::new(EntityKind::Job, "14445");

        reduce(&mut state, AppIntent::SelectEntity(Some(entity.clone())));
        assert_eq!(state.selected_entity.as_ref(), Some(&entity));
    }

    #[test]
    fn read_model_cursor_never_moves_backward() {
        let mut state = FlightDeckState::default();
        reduce(&mut state, AppIntent::ReadModelLoaded { event_cursor: 12 });
        reduce(&mut state, AppIntent::ReadModelLoaded { event_cursor: 9 });
        assert_eq!(state.read_model_cursor, 12);
    }

    #[test]
    fn action_flow_records_execution_and_receipt() {
        let mut state = FlightDeckState::default();
        reduce(
            &mut state,
            AppIntent::BeginActionExecution {
                action_id: "run_tests".into(),
                idempotency_key: "idem-1".into(),
            },
        );
        assert_eq!(
            state.action,
            ActionFlowState::Executing {
                action_id: "run_tests".into(),
                idempotency_key: "idem-1".into(),
            }
        );

        reduce(
            &mut state,
            AppIntent::ActionReceipt {
                receipt_id: "receipt-1".into(),
            },
        );
        assert_eq!(
            state.action,
            ActionFlowState::Receipt {
                receipt_id: "receipt-1".into()
            }
        );
    }
}
