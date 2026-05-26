//! Owner: Tuiwright lens action coverage — llms
//! Proof: `cargo nextest run --test tuiwright -- lenses_llms`
//! Invariants: every key-binding in llms::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::llms::{LENS_ID, LlmsIntent, LlmsLensInput, draw, handle_key};
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

fn input() -> LlmsLensInput {
    LlmsLensInput::from_read_model(&TuiReadModel::default())
}

fn render(w: u16, h: u16, inp: &LlmsLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "llms");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("LLMs"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("LLMs"));
}

#[test]
fn key_enter_drills_selected_call() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        LlmsIntent::DrillSelectedCall
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(handle_key(&k(KeyCode::Esc), &input()), LlmsIntent::PopRoute);
}

#[test]
fn key_b_adjusts_budget() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('b')), &input()),
        LlmsIntent::AdjustBudget
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        LlmsIntent::OpenEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        LlmsIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        LlmsIntent::None
    );
}
