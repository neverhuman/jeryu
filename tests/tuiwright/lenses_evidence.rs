//! Owner: Tuiwright lens action coverage — evidence
//! Proof: `cargo nextest run --test tuiwright -- lenses_evidence`
//! Invariants: every key-binding in evidence::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::evidence::{EvidenceIntent, EvidenceLensInput, LENS_ID, draw, handle_key};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn k(code: KeyCode) -> KeyEvent {
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

fn render(w: u16, h: u16, inp: &EvidenceLensInput) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, inp, f.area())).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[test]
fn lens_id_route_is_kebab() {
    assert_eq!(LENS_ID.route(), "evidence");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Evidence"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Evidence"));
}

#[test]
fn key_enter_drills_selected_proof() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        EvidenceIntent::DrillSelectedProof
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(
        handle_key(&k(KeyCode::Esc), &input()),
        EvidenceIntent::PopRoute
    );
}

#[test]
fn key_slash_opens_filter_query() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('/')), &input()),
        EvidenceIntent::FilterQuery
    );
}

#[test]
fn key_e_opens_selected_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        EvidenceIntent::OpenSelectedEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        EvidenceIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        EvidenceIntent::None
    );
}
