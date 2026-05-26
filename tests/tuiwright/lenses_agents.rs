//! Owner: Tuiwright lens action coverage — agents
//! Proof: `cargo nextest run --test tuiwright -- lenses_agents`
//! Invariants: every key-binding in agents::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::agents::{AgentsIntent, AgentsLensInput, LENS_ID, draw, handle_key};
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

fn input() -> AgentsLensInput {
    AgentsLensInput::from_read_model(&TuiReadModel::default())
}

fn fixture_input() -> AgentsLensInput {
    AgentsLensInput::from_read_model(&fixtures::agents::agent_race())
}

fn render(w: u16, h: u16, inp: &AgentsLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "agents");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Agents"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Agents"));
}

#[test]
fn renders_fixture_input_at_80x24() {
    let ink = render(80, 24, &fixture_input());
    assert!(ink.contains("Agents"));
}

#[test]
fn key_enter_drills_selected_session() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        AgentsIntent::DrillSelectedSession
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(handle_key(&k(KeyCode::Esc), &input()), AgentsIntent::PopRoute);
}

#[test]
fn key_k_rings_kill_bell() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('k')), &input()),
        AgentsIntent::KillBell
    );
}

#[test]
fn key_f_freezes_window() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('f')), &input()),
        AgentsIntent::FreezeWindow
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        AgentsIntent::OpenEvidence
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        AgentsIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        AgentsIntent::None
    );
}
