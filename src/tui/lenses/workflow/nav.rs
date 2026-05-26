//! Owner: Interactive TUI subsystem - Workflow lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::workflow::nav`
//! Invariants: Returns intents only. Never mutates state directly. Enter
//!             drills into the selected pipeline; Esc unwinds; `a` opens
//!             the action menu for the selected pipeline; `e` opens
//!             evidence; `l` opens logs; `n` cycles to the next pipeline
//!             along the critical path; `?` opens contextual help.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::WorkflowLensInput;

/// Intent returned by the workflow lens nav layer. Dispatched by the
/// central reducer (U09-followup). First-cut just defines the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowIntent {
    DrillSelectedPipeline,
    OpenActionMenu,
    OpenEvidence,
    OpenLogs,
    NextPipeline,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &WorkflowLensInput) -> WorkflowIntent {
    match key.code {
        KeyCode::Enter => WorkflowIntent::DrillSelectedPipeline,
        KeyCode::Esc => WorkflowIntent::PopRoute,
        KeyCode::Char('a') => WorkflowIntent::OpenActionMenu,
        KeyCode::Char('e') => WorkflowIntent::OpenEvidence,
        KeyCode::Char('l') => WorkflowIntent::OpenLogs,
        KeyCode::Char('n') => WorkflowIntent::NextPipeline,
        KeyCode::Char('?') => WorkflowIntent::OpenHelp,
        _ => WorkflowIntent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    fn input() -> WorkflowLensInput {
        WorkflowLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills_selected_pipeline() {
        assert_eq!(
            handle_key(&key(KeyCode::Enter), &input()),
            WorkflowIntent::DrillSelectedPipeline
        );
    }

    #[test]
    fn esc_pops_route() {
        assert_eq!(
            handle_key(&key(KeyCode::Esc), &input()),
            WorkflowIntent::PopRoute
        );
    }

    #[test]
    fn a_opens_action_menu() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('a')), &input()),
            WorkflowIntent::OpenActionMenu
        );
    }

    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('e')), &input()),
            WorkflowIntent::OpenEvidence
        );
    }

    #[test]
    fn l_opens_logs() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('l')), &input()),
            WorkflowIntent::OpenLogs
        );
    }

    #[test]
    fn n_cycles_next_pipeline() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('n')), &input()),
            WorkflowIntent::NextPipeline
        );
    }

    #[test]
    fn help_opens_help() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('?')), &input()),
            WorkflowIntent::OpenHelp
        );
    }

    #[test]
    fn unbound_key_returns_none() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('z')), &input()),
            WorkflowIntent::None
        );
    }
}
