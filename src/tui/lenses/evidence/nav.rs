//! Owner: Interactive TUI subsystem - Evidence lens key/mouse navigation
//! Proof: `cargo test -p jeryu --lib tui::lenses::evidence::nav`
//! Invariants: Returns intents only. Never mutates state directly. Enter
//!             drills into the selected proof; Esc unwinds; `/` opens
//!             the filter query; `e` opens the selected evidence
//!             (redundant with Enter, but matches the universal grammar);
//!             `?` opens contextual help.

use crossterm::event::{KeyCode, KeyEvent};

use super::data::EvidenceLensInput;

/// Intent returned by the evidence lens nav layer. Dispatched by the
/// central reducer (U09-followup). First-cut just defines the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceIntent {
    DrillSelectedProof,
    FilterQuery,
    OpenSelectedEvidence,
    OpenHelp,
    PopRoute,
    None,
}

pub fn handle_key(key: &KeyEvent, _input: &EvidenceLensInput) -> EvidenceIntent {
    match key.code {
        KeyCode::Enter => EvidenceIntent::DrillSelectedProof,
        KeyCode::Esc => EvidenceIntent::PopRoute,
        KeyCode::Char('/') => EvidenceIntent::FilterQuery,
        KeyCode::Char('e') => EvidenceIntent::OpenSelectedEvidence,
        KeyCode::Char('?') => EvidenceIntent::OpenHelp,
        _ => EvidenceIntent::None,
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

    fn input() -> EvidenceLensInput {
        EvidenceLensInput::from_read_model(&TuiReadModel::default())
    }

    #[test]
    fn enter_drills_selected_proof() {
        assert_eq!(
            handle_key(&key(KeyCode::Enter), &input()),
            EvidenceIntent::DrillSelectedProof
        );
    }

    #[test]
    fn esc_pops_route() {
        assert_eq!(
            handle_key(&key(KeyCode::Esc), &input()),
            EvidenceIntent::PopRoute
        );
    }

    #[test]
    fn slash_opens_filter_query() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('/')), &input()),
            EvidenceIntent::FilterQuery
        );
    }

    #[test]
    fn e_opens_selected_evidence() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('e')), &input()),
            EvidenceIntent::OpenSelectedEvidence
        );
    }

    #[test]
    fn help_opens_help() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('?')), &input()),
            EvidenceIntent::OpenHelp
        );
    }

    #[test]
    fn unbound_key_returns_none() {
        assert_eq!(
            handle_key(&key(KeyCode::Char('z')), &input()),
            EvidenceIntent::None
        );
    }
}
