//! Owner: Interactive TUI subsystem - Runners lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::runners::nav`
//! Invariants: Returns intents only. Mutations gated by registry preview.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::RunnersLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnersIntent {
    DrillSelectedRunner,
    PausePool,
    DrainPool,
    PreviewScale,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &RunnersLensInput) -> RunnersIntent {
    match key.code {
        KeyCode::Enter => RunnersIntent::DrillSelectedRunner,
        KeyCode::Esc => RunnersIntent::PopRoute,
        KeyCode::Char('p') => RunnersIntent::PausePool,
        KeyCode::Char('d') => RunnersIntent::DrainPool,
        KeyCode::Char('s') => RunnersIntent::PreviewScale,
        KeyCode::Char('e') => RunnersIntent::OpenEvidence,
        KeyCode::Char('?') => RunnersIntent::OpenHelp,
        _ => RunnersIntent::None,
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
    fn input() -> RunnersLensInput {
        RunnersLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills() {
        assert_eq!(handle_key(&k(KeyCode::Enter), &input()), RunnersIntent::DrillSelectedRunner);
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), RunnersIntent::PopRoute);
    }
    #[test]
    fn p_pauses() {
        assert_eq!(handle_key(&k(KeyCode::Char('p')), &input()), RunnersIntent::PausePool);
    }
    #[test]
    fn d_drains() {
        assert_eq!(handle_key(&k(KeyCode::Char('d')), &input()), RunnersIntent::DrainPool);
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(handle_key(&k(KeyCode::Char('z')), &input()), RunnersIntent::None);
    }
}
