//! Owner: Tuiwright lens action coverage — workflow
//! Proof: `cargo nextest run --test tuiwright -- lenses_workflow`
//! Invariants: every key-binding in workflow::nav::handle_key emits the
//!             correct intent; lens render contains its title marker at
//!             80x24 and 120x36.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jeryu::api::read_model::TuiReadModel;
use jeryu::tui::lenses::workflow::{LENS_ID, WorkflowIntent, WorkflowLensInput, draw, handle_key};
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

fn input() -> WorkflowLensInput {
    WorkflowLensInput::from_read_model(&TuiReadModel::default())
}

fn fixture_input() -> WorkflowLensInput {
    WorkflowLensInput::from_read_model(&fixtures::workflow::multi_pipeline())
}

fn render(w: u16, h: u16, inp: &WorkflowLensInput) -> String {
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
    assert_eq!(LENS_ID.route(), "workflow");
}

#[test]
fn renders_title_at_80x24() {
    let ink = render(80, 24, &input());
    assert!(ink.contains("Workflow"));
}

#[test]
fn renders_title_at_120x36() {
    let ink = render(120, 36, &input());
    assert!(ink.contains("Workflow"));
}

#[test]
fn renders_fixture_input_at_80x24() {
    let ink = render(80, 24, &fixture_input());
    assert!(ink.contains("Workflow"));
}

#[test]
fn key_enter_drills_selected_pipeline() {
    assert_eq!(
        handle_key(&k(KeyCode::Enter), &input()),
        WorkflowIntent::DrillSelectedPipeline
    );
}

#[test]
fn key_esc_pops_route() {
    assert_eq!(
        handle_key(&k(KeyCode::Esc), &input()),
        WorkflowIntent::PopRoute
    );
}

#[test]
fn key_a_opens_action_menu() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('a')), &input()),
        WorkflowIntent::OpenActionMenu
    );
}

#[test]
fn key_e_opens_evidence() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('e')), &input()),
        WorkflowIntent::OpenEvidence
    );
}

#[test]
fn key_l_opens_logs() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('l')), &input()),
        WorkflowIntent::OpenLogs
    );
}

#[test]
fn key_n_cycles_next_pipeline() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('n')), &input()),
        WorkflowIntent::NextPipeline
    );
}

#[test]
fn key_question_opens_help() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('?')), &input()),
        WorkflowIntent::OpenHelp
    );
}

#[test]
fn unbound_key_returns_none() {
    assert_eq!(
        handle_key(&k(KeyCode::Char('Z')), &input()),
        WorkflowIntent::None
    );
}
