//! Owner: Interactive TUI subsystem - Approvals lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::approvals::nav`
//! Invariants: Returns intents only. Mutations (approve/reject) are gated by the
//!             action registry preview; this layer never touches backend state.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::ApprovalsLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalsIntent {
    SelectPrev,
    SelectNext,
    ApproveSelected,
    RejectSelected,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &ApprovalsLensInput) -> ApprovalsIntent {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => ApprovalsIntent::SelectPrev,
        KeyCode::Down | KeyCode::Char('j') => ApprovalsIntent::SelectNext,
        KeyCode::Esc => ApprovalsIntent::PopRoute,
        KeyCode::Char('a') => ApprovalsIntent::ApproveSelected,
        KeyCode::Char('r') => ApprovalsIntent::RejectSelected,
        KeyCode::Char('e') => ApprovalsIntent::OpenEvidence,
        KeyCode::Char('?') => ApprovalsIntent::OpenHelp,
        _ => ApprovalsIntent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }
    fn input() -> ApprovalsLensInput {
        ApprovalsLensInput::from_state(&[], 0)
    }

    #[test]
    fn up_selects_prev() {
        assert_eq!(
            handle_key(&k(KeyCode::Up), &input()),
            ApprovalsIntent::SelectPrev
        );
    }
    #[test]
    fn down_selects_next() {
        assert_eq!(
            handle_key(&k(KeyCode::Down), &input()),
            ApprovalsIntent::SelectNext
        );
    }
    #[test]
    fn a_approves() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('a')), &input()),
            ApprovalsIntent::ApproveSelected
        );
    }
    #[test]
    fn r_rejects() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('r')), &input()),
            ApprovalsIntent::RejectSelected
        );
    }
    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('e')), &input()),
            ApprovalsIntent::OpenEvidence
        );
    }
    #[test]
    fn esc_pops() {
        assert_eq!(
            handle_key(&k(KeyCode::Esc), &input()),
            ApprovalsIntent::PopRoute
        );
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('z')), &input()),
            ApprovalsIntent::None
        );
    }
}
