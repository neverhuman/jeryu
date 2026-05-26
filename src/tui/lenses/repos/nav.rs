//! Owner: Interactive TUI subsystem - Repos lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::repos::nav`
//! Invariants: Returns intents only. Never mutates state directly. Enter
//!             drills into the selected family/repo; Esc unwinds; `e`
//!             opens evidence; `?` opens contextual help.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::ReposLensInput;

/// Intent returned by the repos lens nav layer. Dispatched by the central
/// reducer (U09-followup). First-cut just defines the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReposIntent {
    DrillSelectedRepo,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &ReposLensInput) -> ReposIntent {
    match key.code {
        KeyCode::Enter => ReposIntent::DrillSelectedRepo,
        KeyCode::Esc => ReposIntent::PopRoute,
        KeyCode::Char('e') => ReposIntent::OpenEvidence,
        KeyCode::Char('?') => ReposIntent::OpenHelp,
        _ => ReposIntent::None,
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

    fn input() -> ReposLensInput {
        ReposLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills_selected_repo() {
        assert_eq!(
            handle_key(&key(KeyCode::Enter), &input()),
            ReposIntent::DrillSelectedRepo
        );
    }

    #[test]
    fn esc_pops_route() {
        assert_eq!(
            handle_key(&key(KeyCode::Esc), &input()),
            ReposIntent::PopRoute
        );
    }

    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('e')), &input()),
            ReposIntent::OpenEvidence
        );
    }

    #[test]
    fn help_opens_help() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('?')), &input()),
            ReposIntent::OpenHelp
        );
    }

    #[test]
    fn unbound_key_returns_none() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('z')), &input()),
            ReposIntent::None
        );
    }
}
