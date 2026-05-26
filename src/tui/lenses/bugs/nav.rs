//! Owner: Interactive TUI subsystem - Bugs lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::bugs::nav`

use crossterm::event::{KeyCode, KeyEvent};

use super::data::BugsLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BugsIntent {
    DrillSelectedBug,
    SubmitBug,
    RecordAttempt,
    MarkReady,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &BugsLensInput) -> BugsIntent {
    match key.code {
        KeyCode::Enter => BugsIntent::DrillSelectedBug,
        KeyCode::Esc => BugsIntent::PopRoute,
        KeyCode::Char('s') => BugsIntent::SubmitBug,
        KeyCode::Char('r') => BugsIntent::RecordAttempt,
        KeyCode::Char('m') => BugsIntent::MarkReady,
        KeyCode::Char('e') => BugsIntent::OpenEvidence,
        KeyCode::Char('?') => BugsIntent::OpenHelp,
        _ => BugsIntent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::empty(), kind: KeyEventKind::Press, state: KeyEventState::empty() }
    }
    fn input() -> BugsLensInput {
        BugsLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills() {
        assert_eq!(handle_key(&k(KeyCode::Enter), &input()), BugsIntent::DrillSelectedBug);
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), BugsIntent::PopRoute);
    }
    #[test]
    fn s_submits() {
        assert_eq!(handle_key(&k(KeyCode::Char('s')), &input()), BugsIntent::SubmitBug);
    }
    #[test]
    fn m_marks_ready() {
        assert_eq!(handle_key(&k(KeyCode::Char('m')), &input()), BugsIntent::MarkReady);
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(handle_key(&k(KeyCode::Char('z')), &input()), BugsIntent::None);
    }
}
