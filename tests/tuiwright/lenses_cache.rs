//! Owner: Tuiwright lens action coverage — cache
//! Proof: `cargo nextest run --test tuiwright -- lenses_cache`
//! Invariants: every key-binding in cache::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::cache::{CacheIntent, CacheLensInput, LENS_ID, draw, handle_key};
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

fn input() -> CacheLensInput {
    CacheLensInput::from_read_model(&TuiReadModel::default())
}

fn fixture_input() -> CacheLensInput {
    CacheLensInput::from_read_model(&fixtures::cache::cache_pressure())
}

fn render(w: u16, h: u16, inp: &CacheLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "cache");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Cache"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Cache"));
}

#[test]
fn renders_fixture_input_at_80x24() {
    let ink = render(80, 24, &fixture_input());
    assert!(ink.contains("Cache"));
}

#[test]
fn key_enter_drills_selected_object() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        CacheIntent::DrillSelectedObject
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(handle_key(&k(KeyCode::Esc), &input()), CacheIntent::PopRoute);
}

#[test]
fn key_f_flushes_selected() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('f')), &input()),
        CacheIntent::FlushSelected
    );
}

#[test]
fn key_t_marks_taint() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('t')), &input()),
        CacheIntent::MarkTaint
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        CacheIntent::OpenEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        CacheIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        CacheIntent::None
    );
}
