//! Owner: Interactive TUI subsystem - Git lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::git::nav`
//! Invariants: Returns intents only. No mutations. Mirrors the universal lens
//!             keyboard grammar so the recent git-command ledger is navigable
//!             (↑/↓ select · enter detail · e evidence).

use crossterm::event::{KeyCode, KeyEvent};

use super::data::GitLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitIntent {
    SelectPrev,
    SelectNext,
    DrillSelectedEvent,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &GitLensInput) -> GitIntent {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => GitIntent::SelectPrev,
        KeyCode::Down | KeyCode::Char('j') => GitIntent::SelectNext,
        KeyCode::Enter => GitIntent::DrillSelectedEvent,
        KeyCode::Esc => GitIntent::PopRoute,
        KeyCode::Char('e') => GitIntent::OpenEvidence,
        KeyCode::Char('?') => GitIntent::OpenHelp,
        _ => GitIntent::None,
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
    fn input() -> GitLensInput {
        GitLensInput::default()
    }

    #[test]
    fn up_selects_prev() {
        assert_eq!(handle_key(&k(KeyCode::Up), &input()), GitIntent::SelectPrev);
    }
    #[test]
    fn down_selects_next() {
        assert_eq!(
            handle_key(&k(KeyCode::Down), &input()),
            GitIntent::SelectNext
        );
    }
    #[test]
    fn enter_drills() {
        assert_eq!(
            handle_key(&k(KeyCode::Enter), &input()),
            GitIntent::DrillSelectedEvent
        );
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), GitIntent::PopRoute);
    }
    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('e')), &input()),
            GitIntent::OpenEvidence
        );
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('z')), &input()),
            GitIntent::None
        );
    }
}
