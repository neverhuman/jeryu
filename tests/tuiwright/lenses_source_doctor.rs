//! Owner: Tuiwright lens action coverage — source_doctor
//! Proof: `cargo nextest run --test tuiwright -- lenses_source_doctor`
//! Invariants: every key-binding in source_doctor::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::source_doctor::{
    LENS_ID, SourceDoctorIntent, SourceDoctorLensInput, draw, handle_key,
};
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

fn input() -> SourceDoctorLensInput {
    SourceDoctorLensInput::from_read_model(&TuiReadModel::default())
}

fn render(w: u16, h: u16, inp: &SourceDoctorLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "source-doctor");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Source Doctor"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Source Doctor"));
}

#[test]
fn key_enter_drills_selected_source() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        SourceDoctorIntent::DrillSelectedSource
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(
        handle_key(&k(KeyCode::Esc), &input()),
        SourceDoctorIntent::PopRoute
    );
}

#[test]
fn key_e_opens_source_error_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        SourceDoctorIntent::OpenSourceErrorEvidence
    );
}

#[test]
fn key_r_triggers_reconnect_selected() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('r')), &input()),
        SourceDoctorIntent::ReconnectSelected
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        SourceDoctorIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        SourceDoctorIntent::None
    );
}
