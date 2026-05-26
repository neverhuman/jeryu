//! Owner: Interactive TUI subsystem - Mission lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::mission::nav`
//! Invariants: Returns intents only. Never mutates state directly. Enter
//!             drills into the selected attention item; Esc unwinds; `a`
//!             opens the action menu for the selected entity; `e` opens
//!             evidence; `?` opens contextual help.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::MissionLensInput;

/// Intent returned by the mission lens nav layer. These are dispatched to
/// the central reducer (U09-followup) — first-cut just defines the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionIntent {
    DrillTopAttention,
    OpenNextActionMenu,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &MissionLensInput) -> MissionIntent {
    match key.code {
        KeyCode::Enter => MissionIntent::DrillTopAttention,
        KeyCode::Esc => MissionIntent::PopRoute,
        KeyCode::Char('a') => MissionIntent::OpenNextActionMenu,
        KeyCode::Char('e') => MissionIntent::OpenEvidence,
        KeyCode::Char('?') => MissionIntent::OpenHelp,
        _ => MissionIntent::None,
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

    fn input() -> MissionLensInput {
        MissionLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills_top_attention() {
        assert_eq!(handle_key(&key(KeyCode::Enter), &input()), MissionIntent::DrillTopAttention);
    }

    #[test]
    fn esc_pops_route() {
        assert_eq!(handle_key(&key(KeyCode::Esc), &input()), MissionIntent::PopRoute);
    }

    #[test]
    fn a_opens_next_action_menu() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('a')), &input()),
            MissionIntent::OpenNextActionMenu
        );
    }

    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('e')), &input()),
            MissionIntent::OpenEvidence
        );
    }

    #[test]
    fn unbound_key_returns_none() {
        assert_eq!(handle_key(&key(KeyCode::Char('z')), &input()), MissionIntent::None);
    }
}
