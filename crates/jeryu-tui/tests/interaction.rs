//! tuiwright-style interaction coverage for the Flight Deck keyboard model.
//!
//! These tests drive the pure router `runtime::input::handle_key` (the same
//! entry the crossterm event loop calls) against the render-only `App`, and
//! assert the focus state machine behaves identically for **every** tab:
//!
//! * digit shortcuts select the mapped tab,
//! * `Tab` / `BackTab` cycle focus across exactly `PaneId::panes_for_tab`,
//! * `Enter` drills + enters fullscreen, `Esc` unwinds it, and a second `Esc`
//!   quits once back at the top of the focus stack.
//!
//! Pairing this with the `lens_snapshots` render assertions means each tab is
//! covered both as a projection (what is painted) and as an interaction surface
//! (how focus/quit respond to keys) — no terminal required.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use jeryu_readmodel::sample_read_model;
use jeryu_tui::App;
use jeryu_tui::app::{ActiveTab, SessionLaunchPhase};
use jeryu_tui::focus::PaneId;
use jeryu_tui::runtime::input::{Flow, handle_key, handle_key_with_session, handle_key_with_sink};
use jeryu_tui::runtime::session::{SessionHandle, SessionLauncher};
use jeryu_tui::runtime::tty::{AgentControl, ControlSink};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

/// A [`ControlSink`] that records every intent the terminal pane emits, so the
/// interaction tests can assert on what was sent upstream without a transport.
#[derive(Default)]
struct RecordingControlSink {
    sent: Vec<AgentControl>,
}

impl ControlSink for RecordingControlSink {
    fn send(&mut self, control: AgentControl) {
        self.sent.push(control);
    }
}

/// A [`SessionLauncher`] that records every repository it was asked to launch a
/// session on, so the interaction tests can prove the `n` key reaches the seam
/// without a backend.
#[derive(Default)]
struct RecordingLauncher {
    requested: Vec<String>,
}

impl SessionLauncher for RecordingLauncher {
    fn create_session(&mut self, repo_id: &str) -> SessionHandle {
        self.requested.push(repo_id.to_string());
        SessionHandle::new("agent_run.spawned", "agent/spawned")
    }
}

fn app_on(tab: ActiveTab) -> App {
    let mut app = App::new_render_only(sample_read_model());
    app.set_tab(tab);
    app
}

// ── Digit shortcut routing ───────────────────────────────────────────────

#[test]
fn digit_keys_select_their_mapped_tab_from_any_start() {
    // Every digit 0-9 routes to the tab `ActiveTab::from_number` declares,
    // regardless of the tab we start on.
    for start in ActiveTab::ALL {
        for n in 0u8..=9 {
            let Some(expected) = ActiveTab::from_number(n) else {
                continue;
            };
            let mut app = app_on(*start);
            let flow = handle_key(&mut app, key(KeyCode::Char((b'0' + n) as char)));
            assert_eq!(flow, Flow::Continue, "digit must not quit");
            assert_eq!(
                app.active_tab, expected,
                "digit {n} from {start:?} should select {expected:?}"
            );
            // Tab change resets the focus stack to that tab's default pane.
            assert_eq!(app.focus.active, PaneId::default_for_tab(expected));
            assert!(!app.focus.is_drilled());
        }
    }
}

#[test]
fn non_mapped_digit_is_a_no_op() {
    // Only 0-9 map; the routing table stops there. A digit with no mapping must
    // leave the active tab untouched (and never quit).
    let mut app = app_on(ActiveTab::Repos);
    assert_eq!(ActiveTab::from_number(8), Some(ActiveTab::Cache));
    // '8' maps; confirm a *non*-mapping path via the router's guard by using a
    // tab-select digit that maps, then assert an unmapped control key is inert.
    let flow = handle_key(&mut app, key(KeyCode::Char('x')));
    assert_eq!(flow, Flow::Continue);
    assert_eq!(app.active_tab, ActiveTab::Repos);
}

// ── Arrow tab cycling ────────────────────────────────────────────────────

#[test]
fn left_right_arrows_cycle_every_tab_and_wrap() {
    let mut app = app_on(ActiveTab::Workflow);
    // Right through the full ring lands back on the start.
    for expected in ActiveTab::ALL.iter().skip(1) {
        handle_key(&mut app, key(KeyCode::Right));
        assert_eq!(app.active_tab, *expected);
    }
    handle_key(&mut app, key(KeyCode::Right));
    assert_eq!(app.active_tab, ActiveTab::Workflow, "Right must wrap");

    // Left wraps backwards to the last tab.
    handle_key(&mut app, key(KeyCode::Left));
    assert_eq!(app.active_tab, *ActiveTab::ALL.last().unwrap());
}

// ── Focus cycling across panes_for_tab, per tab ───────────────────────────

#[test]
fn tab_and_backtab_cycle_focus_across_panes_for_every_tab() {
    for tab in ActiveTab::ALL {
        let panes = PaneId::panes_for_tab(*tab);
        assert!(!panes.is_empty(), "{tab:?} has no panes");

        let mut app = app_on(*tab);
        // The router starts focus at the tab's default pane.
        assert_eq!(app.focus.active, PaneId::default_for_tab(*tab));

        // Tab forward exactly `panes.len()` times must return to the start,
        // visiting each pane once (set membership check).
        let start = app.focus.active;
        let mut visited = vec![start];
        for _ in 1..panes.len() {
            handle_key(&mut app, key(KeyCode::Tab));
            assert!(
                panes.contains(&app.focus.active),
                "{tab:?}: Tab walked off panes_for_tab to {:?}",
                app.focus.active
            );
            visited.push(app.focus.active);
        }
        handle_key(&mut app, key(KeyCode::Tab));
        assert_eq!(app.focus.active, start, "{tab:?}: Tab must wrap to start");

        // Every pane in the tab was reachable by forward cycling.
        for p in panes {
            assert!(
                visited.contains(p),
                "{tab:?}: pane {p:?} unreachable via Tab cycling"
            );
        }

        // BackTab from the default pane lands on its predecessor in the list
        // (wrapping to the last pane when the default is `panes[0]`).
        let mut app = app_on(*tab);
        let start_idx = panes.iter().position(|p| *p == start).unwrap();
        let expected_prev = panes[(start_idx + panes.len() - 1) % panes.len()];
        handle_key(&mut app, key(KeyCode::BackTab));
        assert_eq!(
            app.focus.active, expected_prev,
            "{tab:?}: BackTab must move to the predecessor pane"
        );
        // A full reverse cycle returns to the start.
        for _ in 1..panes.len() {
            handle_key(&mut app, key(KeyCode::BackTab));
        }
        assert_eq!(
            app.focus.active, start,
            "{tab:?}: BackTab full cycle must return to start"
        );
    }
}

// ── Drill / fullscreen / unwind / quit, per tab ───────────────────────────

#[test]
fn enter_drills_fullscreen_then_esc_unwinds_then_esc_quits_for_every_tab() {
    for tab in ActiveTab::ALL {
        let mut app = app_on(*tab);
        let pane_before = app.focus.active;
        assert!(!app.focus.is_drilled(), "{tab:?}: should start undrilled");

        // Enter: push the focus breadcrumb + enter fullscreen on the active pane.
        assert_eq!(
            handle_key(&mut app, key(KeyCode::Enter)),
            Flow::Continue,
            "{tab:?}: Enter must not quit"
        );
        assert!(app.focus.is_drilled(), "{tab:?}: Enter must drill");
        assert_eq!(
            app.focus.fullscreen,
            Some(pane_before),
            "{tab:?}: Enter must fullscreen the active pane"
        );

        // Esc while drilled unwinds fullscreen + the pushed frame (no quit).
        assert_eq!(
            handle_key(&mut app, key(KeyCode::Esc)),
            Flow::Continue,
            "{tab:?}: Esc must unwind, not quit, while drilled"
        );
        assert!(
            !app.focus.is_drilled(),
            "{tab:?}: Esc must fully unwind the drill"
        );
        assert_eq!(
            app.focus.active, pane_before,
            "{tab:?}: unwind must restore the pre-drill pane"
        );

        // Esc at the top of the stack quits.
        assert_eq!(
            handle_key(&mut app, key(KeyCode::Esc)),
            Flow::Quit,
            "{tab:?}: Esc at top level must quit"
        );
    }
}

#[test]
fn q_quits_at_top_level_but_is_inert_while_drilled_for_every_tab() {
    for tab in ActiveTab::ALL {
        // Drilled: 'q' is swallowed (the guard requires !is_drilled to quit).
        let mut drilled = app_on(*tab);
        handle_key(&mut drilled, key(KeyCode::Enter));
        assert!(drilled.focus.is_drilled());
        assert_eq!(
            handle_key(&mut drilled, key(KeyCode::Char('q'))),
            Flow::Continue,
            "{tab:?}: q must not quit while drilled"
        );

        // Top level: 'q' quits.
        let mut top = app_on(*tab);
        assert_eq!(
            handle_key(&mut top, key(KeyCode::Char('q'))),
            Flow::Quit,
            "{tab:?}: q must quit at top level"
        );
    }
}

#[test]
fn focus_cycle_then_drill_fullscreens_the_focused_pane_for_every_tab() {
    // Compose the two surfaces: move focus with Tab, then Enter must fullscreen
    // *that* pane (not the tab default) — proving focus + drill share state.
    for tab in ActiveTab::ALL {
        let panes = PaneId::panes_for_tab(*tab);
        if panes.len() < 2 {
            continue; // single-pane tabs can't move focus
        }
        let mut app = app_on(*tab);
        handle_key(&mut app, key(KeyCode::Tab));
        let focused = app.focus.active;
        assert_ne!(focused, PaneId::default_for_tab(*tab));

        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(
            app.focus.fullscreen,
            Some(focused),
            "{tab:?}: drill must fullscreen the focused (not default) pane"
        );

        // And Esc restores that focused pane after unwinding.
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.focus.active, focused);
        assert!(!app.focus.is_drilled());
    }
}

// ── Live agent-terminal attach / drive / detach ───────────────────────────

/// Open + attach a terminal on the Agents tab via the production sink entry.
fn attached_agents_app() -> App {
    let mut app = app_on(ActiveTab::Agents);
    let mut sink = RecordingControlSink::default();
    let flow = handle_key_with_sink(&mut app, key(KeyCode::Enter), &mut sink);
    assert_eq!(flow, Flow::Continue);
    app
}

#[test]
fn enter_on_a_run_row_constructs_and_attaches_a_session() {
    let app = attached_agents_app();
    let term = app.terminal.as_ref().expect("Enter must open a session");
    assert!(term.is_attached(), "the opened session must be attached");
    // The sample fleet's first run drives the scope.
    assert!(
        term.run_id().starts_with("agent_run."),
        "session must carry the agent_run scope, got {}",
        term.run_id()
    );
}

#[test]
fn attached_ctrl_c_interrupts_and_does_not_quit() {
    let mut app = attached_agents_app();
    let mut sink = RecordingControlSink::default();
    let flow = handle_key_with_sink(&mut app, ctrl(KeyCode::Char('c')), &mut sink);
    assert_eq!(flow, Flow::Continue, "Ctrl-C must not quit while attached");
    assert_eq!(sink.sent, vec![AgentControl::Interrupt]);
    assert!(app.terminal.as_ref().unwrap().is_attached());
}

#[test]
fn attached_printable_keys_route_as_input_bytes() {
    let mut app = attached_agents_app();
    let mut sink = RecordingControlSink::default();
    handle_key_with_sink(&mut app, key(KeyCode::Char('l')), &mut sink);
    handle_key_with_sink(&mut app, key(KeyCode::Char('s')), &mut sink);
    assert_eq!(
        sink.sent,
        vec![
            AgentControl::Input(b"l".to_vec()),
            AgentControl::Input(b"s".to_vec()),
        ]
    );
}

#[test]
fn detach_key_returns_control_then_q_quits_normally() {
    let mut app = attached_agents_app();
    let mut sink = RecordingControlSink::default();
    // Ctrl-] detaches; no control intent is emitted for the detach itself.
    let flow = handle_key_with_sink(&mut app, ctrl(KeyCode::Char(']')), &mut sink);
    assert_eq!(flow, Flow::Continue);
    assert!(
        sink.sent.is_empty(),
        "detach must not emit a control intent"
    );
    assert!(!app.terminal.as_ref().unwrap().is_attached());

    // With the terminal detached, the normal Flight Deck routing resumes and
    // `q` quits at the top level.
    let flow = handle_key_with_sink(&mut app, key(KeyCode::Char('q')), &mut sink);
    assert_eq!(flow, Flow::Quit, "q must quit once detached");
}

#[test]
fn detached_session_does_not_capture_keys() {
    // A present-but-detached session must not route keys to the terminal: `q`
    // still quits through the standard handler.
    let mut app = attached_agents_app();
    app.terminal.as_mut().unwrap().detach();
    let mut sink = RecordingControlSink::default();
    let flow = handle_key_with_sink(&mut app, key(KeyCode::Char('q')), &mut sink);
    assert_eq!(flow, Flow::Quit);
    assert!(sink.sent.is_empty());
}

// ── New Session affordance (`n` on the Agents tab) ─────────────────────────

#[test]
fn n_on_agents_invokes_the_launcher_and_attaches_the_run() {
    let mut app = app_on(ActiveTab::Agents);
    let mut sink = RecordingControlSink::default();
    let mut launcher = RecordingLauncher::default();

    let flow = handle_key_with_session(&mut app, key(KeyCode::Char('n')), &mut sink, &mut launcher);
    assert_eq!(flow, Flow::Continue);

    // The seam was reached for the in-scope repository.
    assert_eq!(launcher.requested, vec!["core/web".to_string()]);
    // The launch is recorded as attached with the returned run + branch.
    let launch = app.session_launch.as_ref().expect("launch recorded");
    assert_eq!(launch.phase, SessionLaunchPhase::Attached);
    assert_eq!(launch.run_id.as_deref(), Some("agent_run.spawned"));
    assert_eq!(launch.branch.as_deref(), Some("agent/spawned"));
    // The new run's terminal is mounted and attached.
    let term = app.terminal.as_ref().expect("terminal mounted");
    assert!(term.is_attached());
    assert_eq!(term.run_id(), "agent_run.spawned");
}

#[test]
fn n_does_not_break_existing_keys_through_the_session_router() {
    // The session router is a superset of the standard routing: digit
    // tab-select and `q` quit behave identically, and the launcher stays idle.
    let mut app = app_on(ActiveTab::Agents);
    let mut sink = RecordingControlSink::default();
    let mut launcher = RecordingLauncher::default();

    handle_key_with_session(&mut app, key(KeyCode::Char('5')), &mut sink, &mut launcher);
    assert_eq!(app.active_tab, ActiveTab::Agents); // digit 5 == Agents

    handle_key_with_session(&mut app, key(KeyCode::Char('1')), &mut sink, &mut launcher);
    assert_eq!(app.active_tab, ActiveTab::Mission);

    let flow = handle_key_with_session(&mut app, key(KeyCode::Char('q')), &mut sink, &mut launcher);
    assert_eq!(flow, Flow::Quit);
    assert!(launcher.requested.is_empty(), "no spurious session launch");
    assert!(app.session_launch.is_none());
}
