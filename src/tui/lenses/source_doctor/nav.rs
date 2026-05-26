//! Owner: Interactive TUI subsystem - Source Doctor lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::source_doctor::nav`
//! Invariants: Returns intents only. Never mutates state directly. Enter
//!             drills into the selected source; Esc unwinds; `e` opens
//!             source-error evidence; `r` triggers the R1 reconnect
//!             mutation (must route through the action registry + grants
//!             + preview/confirm/execute pipeline); `?` opens contextual
//!             help.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::SourceDoctorLensInput;

/// Intent returned by the source-doctor lens nav layer. Dispatched by
/// the central reducer (U09-followup). First-cut just defines the
/// surface; `ReconnectSelected` is an R1 mutation and must flow through
/// the action registry once U29 proper wires it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceDoctorIntent {
    DrillSelectedSource,
    OpenSourceErrorEvidence,
    ReconnectSelected,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &SourceDoctorLensInput) -> SourceDoctorIntent {
    match key.code {
        KeyCode::Enter => SourceDoctorIntent::DrillSelectedSource,
        KeyCode::Esc => SourceDoctorIntent::PopRoute,
        KeyCode::Char('e') => SourceDoctorIntent::OpenSourceErrorEvidence,
        KeyCode::Char('r') => SourceDoctorIntent::ReconnectSelected,
        KeyCode::Char('?') => SourceDoctorIntent::OpenHelp,
        _ => SourceDoctorIntent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    fn input() -> SourceDoctorLensInput {
        SourceDoctorLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills_selected_source() {
        assert_eq!(
            handle_key(&key(KeyCode::Enter), &input()),
            SourceDoctorIntent::DrillSelectedSource
        );
    }

    #[test]
    fn esc_pops_route() {
        assert_eq!(
            handle_key(&key(KeyCode::Esc), &input()),
            SourceDoctorIntent::PopRoute
        );
    }

    #[test]
    fn e_opens_source_error_evidence() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('e')), &input()),
            SourceDoctorIntent::OpenSourceErrorEvidence
        );
    }

    #[test]
    fn r_triggers_reconnect_selected() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('r')), &input()),
            SourceDoctorIntent::ReconnectSelected
        );
    }

    #[test]
    fn help_opens_help() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('?')), &input()),
            SourceDoctorIntent::OpenHelp
        );
    }

    #[test]
    fn unbound_key_returns_none() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('z')), &input()),
            SourceDoctorIntent::None
        );
    }
}
