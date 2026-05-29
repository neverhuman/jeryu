//! Owner: Interactive TUI subsystem - Secrets lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::secrets::nav`
//! Invariants: Returns intents only; never mutates state directly. Up/Down
//!             move the selection, Enter opens the selected event's detail,
//!             `e` opens the secret-audit evidence trail, Esc unwinds, `?`
//!             opens contextual help. SECURITY: navigation surfaces audit
//!             metadata only — no intent ever reveals a secret value.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::SecretsLensInput;

/// Intent returned by the secrets lens nav layer. Dispatched by the central
/// reducer; this layer is pure and side-effect free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretsIntent {
    SelectPrev,
    SelectNext,
    OpenSelectedDetail,
    OpenEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &SecretsLensInput) -> SecretsIntent {
    match key.code {
        KeyCode::Up => SecretsIntent::SelectPrev,
        KeyCode::Down => SecretsIntent::SelectNext,
        KeyCode::Enter => SecretsIntent::OpenSelectedDetail,
        KeyCode::Char('e') => SecretsIntent::OpenEvidence,
        KeyCode::Char('?') => SecretsIntent::OpenHelp,
        KeyCode::Esc => SecretsIntent::PopRoute,
        _ => SecretsIntent::None,
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

    fn input() -> SecretsLensInput {
        SecretsLensInput::from_state(&[], 0)
    }

    #[test]
    fn up_selects_prev() {
        assert_eq!(
            handle_key(&k(KeyCode::Up), &input()),
            SecretsIntent::SelectPrev
        );
    }

    #[test]
    fn down_selects_next() {
        assert_eq!(
            handle_key(&k(KeyCode::Down), &input()),
            SecretsIntent::SelectNext
        );
    }

    #[test]
    fn enter_opens_detail() {
        assert_eq!(
            handle_key(&k(KeyCode::Enter), &input()),
            SecretsIntent::OpenSelectedDetail
        );
    }

    #[test]
    fn e_opens_evidence() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('e')), &input()),
            SecretsIntent::OpenEvidence
        );
    }

    #[test]
    fn esc_pops_route() {
        assert_eq!(
            handle_key(&k(KeyCode::Esc), &input()),
            SecretsIntent::PopRoute
        );
    }

    #[test]
    fn help_opens_help() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('?')), &input()),
            SecretsIntent::OpenHelp
        );
    }

    #[test]
    fn unbound_returns_none() {
        assert_eq!(
            handle_key(&k(KeyCode::Char('z')), &input()),
            SecretsIntent::None
        );
    }
}
