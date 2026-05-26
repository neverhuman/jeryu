//! Owner: Interactive TUI subsystem - Release lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::release::nav`
//! Invariants: Returns intents only. Mutations gated by typed
//!             confirmation. PromoteCandidate is R4 (production);
//!             Rollback is R4 typed; ApproveGate is R3 (repo).

use crossterm::event::{KeyCode, KeyEvent};

use super::data::ReleaseLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseIntent {
    PromoteCandidate,
    Rollback,
    ApproveGate,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &ReleaseLensInput) -> ReleaseIntent {
    match key.code {
        KeyCode::Enter => ReleaseIntent::PromoteCandidate,
        KeyCode::Esc => ReleaseIntent::PopRoute,
        KeyCode::Char('r') => ReleaseIntent::Rollback,
        KeyCode::Char('a') => ReleaseIntent::ApproveGate,
        KeyCode::Char('e') => ReleaseIntent::OpenEvidence,
        KeyCode::Char('?') => ReleaseIntent::OpenHelp,
        _ => ReleaseIntent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }
    fn input() -> ReleaseLensInput {
        ReleaseLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_promotes() {
        assert_eq!(
            handle_key(&k(KeyCode::Enter), &input()),
            ReleaseIntent::PromoteCandidate
        );
    }
    #[test]
    fn esc_pops() {
        assert_eq!(
            handle_key(&k(KeyCode::Esc), &input()),
            ReleaseIntent::PopRoute
        );
    }
    #[test]
    fn r_rolls_back() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('r')), &input()),
            ReleaseIntent::Rollback
        );
    }
    #[test]
    fn a_approves_gate() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('a')), &input()),
            ReleaseIntent::ApproveGate
        );
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('z')), &input()),
            ReleaseIntent::None
        );
    }
}
