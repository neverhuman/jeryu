//! Owner: Tuiwright lens action coverage — autonomy
//! Proof: `cargo nextest run --test tuiwright -- lenses_autonomy`
//! Invariants: every key-binding in autonomy::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.
//!
//! Note: the user-supplied binding table did not include an explicit row
//! for the Autonomy lens. The actual surface (read from
//! src/tui/lenses/autonomy/nav.rs) is: Enter→GrantApprove, Esc→PopRoute,
//! k→KillBell, f→Freeze, e→OpenEvidence, ?→OpenHelp.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::autonomy::{AutonomyIntent, AutonomyLensInput, LENS_ID, draw, handle_key};
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

fn input() -> AutonomyLensInput {
    AutonomyLensInput::from_read_model(&TuiReadModel::default())
}

fn render(w: u16, h: u16, inp: &AutonomyLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "autonomy");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Autonomy"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Autonomy"));
}

#[test]
fn key_enter_approves_grant() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        AutonomyIntent::GrantApprove
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(
        handle_key(&k(KeyCode::Esc), &input()),
        AutonomyIntent::PopRoute
    );
}

#[test]
fn key_k_rings_kill_bell() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('k')), &input()),
        AutonomyIntent::KillBell
    );
}

#[test]
fn key_f_freezes() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('f')), &input()),
        AutonomyIntent::Freeze
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        AutonomyIntent::OpenEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        AutonomyIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        AutonomyIntent::None
    );
}
