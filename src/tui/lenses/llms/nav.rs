//! Owner: Interactive TUI subsystem - LLMs lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::llms::nav`
//! Invariants: Returns intents only. AdjustBudget is R3 (repo) and
//!             gated by registry preview.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::LlmsLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmsIntent {
    DrillSelectedCall,
    AdjustBudget,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &LlmsLensInput) -> LlmsIntent {
    match key.code {
        KeyCode::Enter => LlmsIntent::DrillSelectedCall,
        KeyCode::Esc => LlmsIntent::PopRoute,
        KeyCode::Char('b') => LlmsIntent::AdjustBudget,
        KeyCode::Char('e') => LlmsIntent::OpenEvidence,
        KeyCode::Char('?') => LlmsIntent::OpenHelp,
        _ => LlmsIntent::None,
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
    fn input() -> LlmsLensInput {
        LlmsLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills() {
        assert_eq!(
            handle_key(&k(KeyCode::Enter), &input()),
            LlmsIntent::DrillSelectedCall
        );
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), LlmsIntent::PopRoute);
    }
    #[test]
    fn b_adjusts_budget() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('b')), &input()),
            LlmsIntent::AdjustBudget
        );
    }
    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('e')), &input()),
            LlmsIntent::OpenEvidence
        );
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('z')), &input()),
            LlmsIntent::None
        );
    }
}
