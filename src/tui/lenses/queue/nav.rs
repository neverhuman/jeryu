//! Owner: Interactive TUI subsystem - Queue lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::queue::nav`
//! Invariants: Returns intents only. Never mutates state directly. Enter
//!             drills into the top waiting job; Esc unwinds; `a` opens
//!             the scale-pool menu for the selected pool; `e` opens
//!             evidence; `?` opens contextual help.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::QueueLensInput;

/// Intent returned by the queue lens nav layer. Dispatched by the central
/// reducer (U09-followup). First-cut just defines the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueIntent {
    DrillTopWaitingJob,
    OpenScalePoolMenu,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &QueueLensInput) -> QueueIntent {
    match key.code {
        KeyCode::Enter => QueueIntent::DrillTopWaitingJob,
        KeyCode::Esc => QueueIntent::PopRoute,
        KeyCode::Char('a') => QueueIntent::OpenScalePoolMenu,
        KeyCode::Char('e') => QueueIntent::OpenEvidence,
        KeyCode::Char('?') => QueueIntent::OpenHelp,
        _ => QueueIntent::None,
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

    fn input() -> QueueLensInput {
        QueueLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills_top_waiting_job() {
        assert_eq!(
            handle_key(&key(KeyCode::Enter), &input()),
            QueueIntent::DrillTopWaitingJob
        );
    }

    #[test]
    fn esc_pops_route() {
        assert_eq!(handle_key(&key(KeyCode::Esc), &input()), QueueIntent::PopRoute);
    }

    #[test]
    fn a_opens_scale_pool_menu() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('a')), &input()),
            QueueIntent::OpenScalePoolMenu
        );
    }

    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('e')), &input()),
            QueueIntent::OpenEvidence
        );
    }

    #[test]
    fn help_opens_help() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('?')), &input()),
            QueueIntent::OpenHelp
        );
    }

    #[test]
    fn unbound_key_returns_none() {
        assert_eq!(handle_key(&key(KeyCode::Char('z')), &input()), QueueIntent::None);
    }
}
