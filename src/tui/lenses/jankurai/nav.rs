//! Owner: Interactive TUI subsystem - Jankurai lens key navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::jankurai::nav`
//! Invariants: Returns intents only; never mutates state.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::JankuraiLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JankuraiIntent {
    SelectPrev,
    SelectNext,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &JankuraiLensInput) -> JankuraiIntent {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => JankuraiIntent::SelectPrev,
        KeyCode::Down | KeyCode::Char('j') => JankuraiIntent::SelectNext,
        KeyCode::Char('e') => JankuraiIntent::OpenEvidence,
        KeyCode::Char('?') => JankuraiIntent::OpenHelp,
        KeyCode::Esc => JankuraiIntent::PopRoute,
        _ => JankuraiIntent::None,
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

    fn input() -> JankuraiLensInput {
        JankuraiLensInput::default()
    }

    #[test]
    fn up_selects_prev() {
        assert_eq!(
            handle_key(&k(KeyCode::Up), &input()),
            JankuraiIntent::SelectPrev
        );
    }

    #[test]
    fn down_selects_next() {
        assert_eq!(
            handle_key(&k(KeyCode::Down), &input()),
            JankuraiIntent::SelectNext
        );
    }

    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('e')), &input()),
            JankuraiIntent::OpenEvidence
        );
    }

    #[test]
    fn esc_pops() {
        assert_eq!(
            handle_key(&k(KeyCode::Esc), &input()),
            JankuraiIntent::PopRoute
        );
    }

    #[test]
    fn unbound_returns_none() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('z')), &input()),
            JankuraiIntent::None
        );
    }
}
