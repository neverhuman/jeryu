//! Owner: Interactive TUI subsystem - Agents lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::agents::nav`

use crossterm::event::{KeyCode, KeyEvent};

use super::data::AgentsLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentsIntent {
    DrillSelectedSession,
    KillBell,
    FreezeWindow,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &AgentsLensInput) -> AgentsIntent {
    match key.code {
        KeyCode::Enter => AgentsIntent::DrillSelectedSession,
        KeyCode::Esc => AgentsIntent::PopRoute,
        KeyCode::Char('k') => AgentsIntent::KillBell,
        KeyCode::Char('f') => AgentsIntent::FreezeWindow,
        KeyCode::Char('e') => AgentsIntent::OpenEvidence,
        KeyCode::Char('?') => AgentsIntent::OpenHelp,
        _ => AgentsIntent::None,
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
    fn input() -> AgentsLensInput {
        AgentsLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills() {
        assert_eq!(handle_key(&k(KeyCode::Enter), &input()), AgentsIntent::DrillSelectedSession);
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), AgentsIntent::PopRoute);
    }
    #[test]
    fn k_rings_kill_bell() {
        assert_eq!(handle_key(&k(KeyCode::Char('k')), &input()), AgentsIntent::KillBell);
    }
    #[test]
    fn f_freezes_window() {
        assert_eq!(handle_key(&k(KeyCode::Char('f')), &input()), AgentsIntent::FreezeWindow);
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(handle_key(&k(KeyCode::Char('z')), &input()), AgentsIntent::None);
    }
}
