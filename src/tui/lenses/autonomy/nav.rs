//! Owner: Interactive TUI subsystem - Autonomy lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::autonomy::nav`
//! Invariants: Returns intents only. KillBell is R5 (irreversible);
//!             Freeze and GrantApprove are R3 (repo). All mutations gated
//!             by registry preview.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::AutonomyLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomyIntent {
    KillBell,
    Freeze,
    GrantApprove,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &AutonomyLensInput) -> AutonomyIntent {
    match key.code {
        KeyCode::Enter => AutonomyIntent::GrantApprove,
        KeyCode::Esc => AutonomyIntent::PopRoute,
        KeyCode::Char('k') => AutonomyIntent::KillBell,
        KeyCode::Char('f') => AutonomyIntent::Freeze,
        KeyCode::Char('e') => AutonomyIntent::OpenEvidence,
        KeyCode::Char('?') => AutonomyIntent::OpenHelp,
        _ => AutonomyIntent::None,
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
    fn input() -> AutonomyLensInput {
        AutonomyLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_approves_grant() {
        assert_eq!(handle_key(&k(KeyCode::Enter), &input()), AutonomyIntent::GrantApprove);
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), AutonomyIntent::PopRoute);
    }
    #[test]
    fn k_kill_bells() {
        assert_eq!(handle_key(&k(KeyCode::Char('k')), &input()), AutonomyIntent::KillBell);
    }
    #[test]
    fn f_freezes() {
        assert_eq!(handle_key(&k(KeyCode::Char('f')), &input()), AutonomyIntent::Freeze);
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(handle_key(&k(KeyCode::Char('z')), &input()), AutonomyIntent::None);
    }
}
