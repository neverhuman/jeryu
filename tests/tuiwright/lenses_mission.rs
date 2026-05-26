//! Owner: Tuiwright lens action coverage — mission
//! Proof: `cargo nextest run --test tuiwright -- lenses_mission`
//! Invariants: every key-binding in mission::nav::handle_key emits the correct
//!             intent; lens render contains its title marker at 80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::mission::{
    LENS_ID, MissionIntent, MissionLensInput, draw, handle_key,
};
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

fn input() -> MissionLensInput {
    MissionLensInput::from_read_model(&TuiReadModel::default())
}

fn fixture_input() -> MissionLensInput {
    MissionLensInput::from_read_model(&fixtures::mission::degraded())
}

fn render(w: u16, h: u16, inp: &MissionLensInput) -> String {
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
fn lens_id_route_is_nonempty() {
    assert_eq!(LENS_ID.route(), "mission");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Posture"), "missing Posture in render");
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Posture"));
}

#[test]
fn renders_fixture_input_at_80x24() {
    let ink = render(80, 24, &fixture_input());
    assert!(ink.contains("Posture"));
}

#[test]
fn key_enter_drills_top_attention() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        MissionIntent::DrillTopAttention
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(handle_key(&k(KeyCode::Esc), &input()), MissionIntent::PopRoute);
}

#[test]
fn key_a_opens_next_action_menu() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('a')), &input()),
        MissionIntent::OpenNextActionMenu
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        MissionIntent::OpenEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        MissionIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        MissionIntent::None
    );
}
