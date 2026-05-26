//! Owner: Tuiwright lens action coverage — runners
//! Proof: `cargo nextest run --test tuiwright -- lenses_runners`
//! Invariants: every key-binding in runners::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::runners::{LENS_ID, RunnersIntent, RunnersLensInput, draw, handle_key};
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

fn input() -> RunnersLensInput {
    RunnersLensInput::from_read_model(&TuiReadModel::default())
}

fn render(w: u16, h: u16, inp: &RunnersLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "runners");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Runners"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Runners"));
}

#[test]
fn key_enter_drills_selected_runner() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        RunnersIntent::DrillSelectedRunner
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(
        handle_key(&k(KeyCode::Esc), &input()),
        RunnersIntent::PopRoute
    );
}

#[test]
fn key_p_pauses_pool() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('p')), &input()),
        RunnersIntent::PausePool
    );
}

#[test]
fn key_d_drains_pool() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('d')), &input()),
        RunnersIntent::DrainPool
    );
}

#[test]
fn key_s_previews_scale() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('s')), &input()),
        RunnersIntent::PreviewScale
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        RunnersIntent::OpenEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        RunnersIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        RunnersIntent::None
    );
}
