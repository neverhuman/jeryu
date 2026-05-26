//! Owner: Interactive TUI subsystem - Cache lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::cache::nav`
//! Invariants: Returns intents only. Mutations gated by registry preview.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::CacheLensInput;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheIntent {
    DrillSelectedObject,
    FlushSelected,
    MarkTaint,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &CacheLensInput) -> CacheIntent {
    match key.code {
        KeyCode::Enter => CacheIntent::DrillSelectedObject,
        KeyCode::Esc => CacheIntent::PopRoute,
        KeyCode::Char('f') => CacheIntent::FlushSelected,
        KeyCode::Char('t') => CacheIntent::MarkTaint,
        KeyCode::Char('e') => CacheIntent::OpenEvidence,
        KeyCode::Char('?') => CacheIntent::OpenHelp,
        _ => CacheIntent::None,
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
    fn input() -> CacheLensInput {
        CacheLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills() {
        assert_eq!(handle_key(&k(KeyCode::Enter), &input()), CacheIntent::DrillSelectedObject);
    }
    #[test]
    fn esc_pops() {
        assert_eq!(handle_key(&k(KeyCode::Esc), &input()), CacheIntent::PopRoute);
    }
    #[test]
    fn f_flushes() {
        assert_eq!(handle_key(&k(KeyCode::Char('f')), &input()), CacheIntent::FlushSelected);
    }
    #[test]
    fn t_marks_taint() {
        assert_eq!(handle_key(&k(KeyCode::Char('t')), &input()), CacheIntent::MarkTaint);
    }
    #[test]
    fn unbound_returns_none() {
        assert_eq!(handle_key(&k(KeyCode::Char('z')), &input()), CacheIntent::None);
    }
}
