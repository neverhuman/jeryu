//! Owner: Tuiwright lens action coverage — vti
//! Proof: `cargo nextest run --test tuiwright -- lenses_vti`
//! Invariants: every key-binding in vti::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::vti::{LENS_ID, VtiIntent, VtiLensInput, draw, handle_key};
use jeryu::tui::testing::fixtures;
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

fn input() -> VtiLensInput {
    VtiLensInput::from_read_model(&TuiReadModel::default())
}

fn fixture_input() -> VtiLensInput {
    VtiLensInput::from_read_model(&fixtures::vti::vti_miss())
}

fn render(w: u16, h: u16, inp: &VtiLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "vti");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("VTI"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("VTI"));
}

#[test]
fn renders_fixture_input_at_80x24() {
    let ink = render(80, 24, &fixture_input());
    assert!(ink.contains("VTI"));
}

#[test]
fn key_enter_drills_selected_plan() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        VtiIntent::DrillSelectedPlan
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(handle_key(&k(KeyCode::Esc), &input()), VtiIntent::PopRoute);
}

#[test]
fn key_f_forces_full_run() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('f')), &input()),
        VtiIntent::ForceFullRun
    );
}

#[test]
fn key_q_quarantines() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('q')), &input()),
        VtiIntent::Quarantine
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        VtiIntent::OpenEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        VtiIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        VtiIntent::None
    );
}
