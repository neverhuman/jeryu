//! Owner: Interactive TUI subsystem - VTI lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::vti::nav`
//! Invariants: Returns intents only. Mutations gated by registry preview.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::VtiLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VtiIntent {
    DrillSelectedPlan,
    ForceFullRun,
    Quarantine,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &VtiLensInput) -> VtiIntent {
    match key.code {
        KeyCode::Enter => VtiIntent::DrillSelectedPlan,
        KeyCode::Esc => VtiIntent::PopRoute,
        KeyCode::Char('f') => VtiIntent::ForceFullRun,
        KeyCode::Char('q') => VtiIntent::Quarantine,
        KeyCode::Char('e') => VtiIntent::OpenEvidence,
        KeyCode::Char('?') => VtiIntent::OpenHelp,
        _ => VtiIntent::None,
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
    fn input() -> VtiLensInput {
        VtiLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills() {
        assert_eq!(
            handle_key(&k(KeyCode::Enter), &input()),
            VtiIntent::DrillSelectedPlan
        );
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), VtiIntent::PopRoute);
    }
    #[test]
    fn f_forces_full_run() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('f')), &input()),
            VtiIntent::ForceFullRun
        );
    }
    #[test]
    fn q_quarantines() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('q')), &input()),
            VtiIntent::Quarantine
        );
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('z')), &input()),
            VtiIntent::None
        );
    }
}
