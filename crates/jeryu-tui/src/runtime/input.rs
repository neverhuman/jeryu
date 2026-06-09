//! Event-loop scaffold: keyboard routing for the Flight Deck.
//!
//! The real product drives this from a crossterm event stream; here the routing
//! is factored into a pure [`handle_key`] so it is unit-testable without a
//! terminal. The event loop itself ([`run_loop`]) is a thin wrapper a binary
//! would call; it is not exercised by the standalone test suite (no TTY).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{ActiveTab, App, SessionLaunch};
use crate::lenses::agents::AgentTerminalSession;
use crate::runtime::session::SessionLauncher;
use crate::runtime::tty::{AgentControl, ControlSink, TtySource};

/// Default emulator grid for a freshly opened agent terminal. The pane resizes
/// to the live geometry on the first resize event.
const DEFAULT_TERMINAL_ROWS: u16 = 24;
const DEFAULT_TERMINAL_COLS: u16 = 80;

/// Result of routing a single key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep running; redraw on the next tick.
    Continue,
    /// Quit the event loop.
    Quit,
}

/// Route one key event against the app state. Pure: mutates `app`, returns the
/// control-flow decision. No I/O.
pub fn handle_key(app: &mut App, key: KeyEvent) -> Flow {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc if !app.focus.is_drilled() => Flow::Quit,
        KeyCode::Esc => {
            app.focus.escape();
            Flow::Continue
        }
        KeyCode::Tab => {
            app.focus.focus_next(app.active_tab);
            Flow::Continue
        }
        KeyCode::BackTab => {
            app.focus.focus_prev(app.active_tab);
            Flow::Continue
        }
        KeyCode::Right => {
            app.set_tab(app.active_tab.next());
            Flow::Continue
        }
        KeyCode::Left => {
            app.set_tab(app.active_tab.prev());
            Flow::Continue
        }
        KeyCode::Enter => {
            app.focus.push();
            app.focus.enter_fullscreen();
            Flow::Continue
        }
        KeyCode::Char(c @ '0'..='9') => {
            if let Some(tab) = ActiveTab::from_number(c as u8 - b'0') {
                app.set_tab(tab);
            }
            Flow::Continue
        }
        _ => Flow::Continue,
    }
}

/// Route one key event against the app state, sending any terminal-control
/// intents through `sink`. This is the entry the production event loop calls.
///
/// Routing precedence:
/// 1. An attached agent terminal owns the keyboard — keys flow to
///    [`handle_terminal_key`] (and `q`/`Esc` do **not** quit while attached).
/// 2. On the Agents tab, `Enter` opens + attaches a live terminal on the
///    selected run (additive to the focus model).
/// 3. Otherwise the standard Flight Deck routing in [`handle_key`] applies.
pub fn handle_key_with_sink(app: &mut App, key: KeyEvent, sink: &mut impl ControlSink) -> Flow {
    if app
        .terminal
        .as_ref()
        .is_some_and(AgentTerminalSession::is_attached)
    {
        return handle_terminal_key(app, key, sink);
    }

    if app.active_tab == ActiveTab::Agents
        && key.code == KeyCode::Enter
        && let Some(run_id) = selected_run_id(app)
    {
        let mut session =
            AgentTerminalSession::new(run_id, DEFAULT_TERMINAL_ROWS, DEFAULT_TERMINAL_COLS);
        session.attach();
        app.terminal = Some(session);
        return Flow::Continue;
    }

    handle_key(app, key)
}

/// Route one key event with both a control sink and a [`SessionLauncher`].
///
/// This is the full Agents-lens entry: it adds the "New Session" affordance on
/// top of [`handle_key_with_sink`]. On the Agents tab, `n` creates a fresh,
/// isolated agent session through `launcher`, records the launch on
/// [`App::session_launch`], and attaches its live terminal — mirroring the web
/// `RepositoryAgentsPage` flow (POST `/repos/{id}/sessions` → mount the
/// terminal on the returned run). Every other key falls through to
/// [`handle_key_with_sink`], so existing routing is unchanged.
pub fn handle_key_with_session(
    app: &mut App,
    key: KeyEvent,
    sink: &mut impl ControlSink,
    launcher: &mut impl SessionLauncher,
) -> Flow {
    let attached = app
        .terminal
        .as_ref()
        .is_some_and(AgentTerminalSession::is_attached);
    if !attached
        && app.active_tab == ActiveTab::Agents
        && key.code == KeyCode::Char('n')
        && let Some(repo_id) = current_repo_id(app)
    {
        launch_session(app, &repo_id, launcher);
        return Flow::Continue;
    }

    handle_key_with_sink(app, key, sink)
}

/// Create an isolated session on `repo_id` and attach its live terminal. The
/// launch is marked pending the instant the intent fires, then attached once
/// the launcher hands back the run handle.
fn launch_session(app: &mut App, repo_id: &str, launcher: &mut impl SessionLauncher) {
    app.session_launch = Some(SessionLaunch::pending(repo_id));
    let handle = launcher.create_session(repo_id);
    let mut session = AgentTerminalSession::new(
        handle.run_id.clone(),
        DEFAULT_TERMINAL_ROWS,
        DEFAULT_TERMINAL_COLS,
    );
    session.attach();
    app.terminal = Some(session);
    app.session_launch = Some(SessionLaunch::attach(repo_id, &handle));
}

/// The repository a new session targets: the first repo in the read model's
/// repos snapshot. `None` when no repository is in scope, so `n` is inert
/// exactly like `Enter` is when no run is selectable.
fn current_repo_id(app: &App) -> Option<String> {
    app.model
        .repos
        .repos
        .first()
        .map(|repo| repo.entity.id.clone())
}

/// Route a key while an agent terminal is attached. Keystrokes are encoded as
/// terminal input; Ctrl-C interrupts the foreground process (it does **not**
/// quit the Flight Deck); Ctrl-] detaches and returns control to the lens.
pub fn handle_terminal_key(app: &mut App, key: KeyEvent, sink: &mut impl ControlSink) -> Flow {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        // Ctrl-] detaches: the lens regains the keyboard.
        KeyCode::Char(']') if ctrl => {
            if let Some(term) = app.terminal.as_mut() {
                term.detach();
            }
            Flow::Continue
        }
        // Ctrl-C interrupts the agent's foreground process without quitting.
        KeyCode::Char('c') if ctrl => {
            sink.send(AgentControl::Interrupt);
            Flow::Continue
        }
        KeyCode::Char(c) => {
            sink.send(AgentControl::Input(encode_char(c, key.modifiers)));
            Flow::Continue
        }
        KeyCode::Enter => {
            sink.send(AgentControl::Input(b"\r".to_vec()));
            Flow::Continue
        }
        KeyCode::Tab => {
            sink.send(AgentControl::Input(b"\t".to_vec()));
            Flow::Continue
        }
        KeyCode::Backspace => {
            sink.send(AgentControl::Input(vec![0x7f]));
            Flow::Continue
        }
        KeyCode::Esc => {
            sink.send(AgentControl::Input(vec![0x1b]));
            Flow::Continue
        }
        _ => Flow::Continue,
    }
}

/// Encode a printable key into terminal input bytes, folding `Ctrl`+letter into
/// its C0 control byte.
fn encode_char(c: char, modifiers: KeyModifiers) -> Vec<u8> {
    if modifiers.contains(KeyModifiers::CONTROL) && c.is_ascii_alphabetic() {
        return vec![(c.to_ascii_lowercase() as u8) & 0x1f];
    }
    c.to_string().into_bytes()
}

/// Drain every chunk available from `source` into `session`, feeding the
/// emulator in sequence and asking the source for a [`AgentControl::Resync`]
/// whenever a `chunk_seq` gap is detected (which also flips the session to
/// lagged). This is the production drain loop the `WsTtyBridge` will call each
/// tick; the tests drive it with a [`ScriptedTtySource`](crate::runtime::tty).
pub fn pump_terminal(
    session: &mut AgentTerminalSession,
    source: &mut impl TtySource,
    sink: &mut impl ControlSink,
) {
    for chunk in source.poll() {
        if session.observe_chunk_seq(chunk.chunk_seq) {
            sink.send(AgentControl::Resync);
        }
        session.feed(&chunk.bytes);
    }
}

/// The agent run selected on the Agents tab, if any. A run is selectable when
/// the read model carries at least one agent session.
fn selected_run_id(app: &App) -> Option<String> {
    app.model
        .agents
        .items
        .first()
        .map(|item| format!("agent_run.{}", item.session_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn q_quits_when_not_drilled() {
        let mut app = App::default();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('q'))), Flow::Quit);
    }

    #[test]
    fn digit_selects_tab() {
        let mut app = App::default();
        handle_key(&mut app, key(KeyCode::Char('1')));
        assert_eq!(app.active_tab, ActiveTab::Mission);
        handle_key(&mut app, key(KeyCode::Char('9')));
        assert_eq!(app.active_tab, ActiveTab::Evidence);
    }

    #[test]
    fn arrows_cycle_tabs() {
        let mut app = App::default(); // Workflow
        handle_key(&mut app, key(KeyCode::Right));
        assert_eq!(app.active_tab, ActiveTab::Mission);
        handle_key(&mut app, key(KeyCode::Left));
        assert_eq!(app.active_tab, ActiveTab::Workflow);
    }

    #[test]
    fn enter_drills_and_esc_unwinds_instead_of_quitting() {
        let mut app = App::default();
        assert_eq!(handle_key(&mut app, key(KeyCode::Enter)), Flow::Continue);
        assert!(app.focus.is_drilled());
        // Esc while drilled unwinds, does not quit.
        assert_eq!(handle_key(&mut app, key(KeyCode::Esc)), Flow::Continue);
        assert!(!app.focus.is_drilled());
        // Esc at top level quits.
        assert_eq!(handle_key(&mut app, key(KeyCode::Esc)), Flow::Quit);
    }

    #[test]
    fn tab_cycles_focus_within_tab() {
        let mut app = App::default();
        app.set_tab(ActiveTab::Mission);
        let first = app.focus.active;
        handle_key(&mut app, key(KeyCode::Tab));
        assert_ne!(app.focus.active, first);
    }

    // ── Terminal routing ──────────────────────────────────────────────────

    use crate::app::SessionLaunchPhase;
    use crate::runtime::session::RecordingSessionLauncher;
    use crate::runtime::tty::{RecordingControlSink, ScriptedTtySource, TtyChunk};
    use jeryu_readmodel::sample_read_model;

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn agents_app() -> App {
        let mut app = App::new_render_only(sample_read_model());
        app.set_tab(ActiveTab::Agents);
        app
    }

    #[test]
    fn enter_on_agents_opens_and_attaches_a_session() {
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        assert_eq!(
            handle_key_with_sink(&mut app, key(KeyCode::Enter), &mut sink),
            Flow::Continue
        );
        let term = app.terminal.as_ref().expect("session opened");
        assert!(term.is_attached());
        assert!(term.run_id().starts_with("agent_run."));
        assert!(sink.sent.is_empty());
    }

    #[test]
    fn attached_ctrl_c_emits_interrupt_without_quitting() {
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        handle_key_with_sink(&mut app, key(KeyCode::Enter), &mut sink);
        assert_eq!(
            handle_key_with_sink(&mut app, ctrl(KeyCode::Char('c')), &mut sink),
            Flow::Continue
        );
        assert_eq!(sink.sent, vec![AgentControl::Interrupt]);
    }

    #[test]
    fn attached_printable_keys_become_input_bytes() {
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        handle_key_with_sink(&mut app, key(KeyCode::Enter), &mut sink);
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
    fn ctrl_letter_folds_to_c0_control_byte() {
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        handle_key_with_sink(&mut app, key(KeyCode::Enter), &mut sink);
        // Ctrl-A encodes as 0x01.
        handle_key_with_sink(&mut app, ctrl(KeyCode::Char('a')), &mut sink);
        assert_eq!(sink.sent, vec![AgentControl::Input(vec![0x01])]);
    }

    #[test]
    fn detach_key_releases_keyboard_and_q_then_quits() {
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        handle_key_with_sink(&mut app, key(KeyCode::Enter), &mut sink);
        handle_key_with_sink(&mut app, ctrl(KeyCode::Char(']')), &mut sink);
        assert!(!app.terminal.as_ref().unwrap().is_attached());
        assert!(sink.sent.is_empty());
        assert_eq!(
            handle_key_with_sink(&mut app, key(KeyCode::Char('q')), &mut sink),
            Flow::Quit
        );
    }

    #[test]
    fn pump_feeds_chunks_in_order_and_resyncs_on_gap() {
        let mut session = AgentTerminalSession::new("agent_run.1", 24, 80);
        let mut source = ScriptedTtySource::new(vec![
            TtyChunk {
                chunk_seq: 1,
                bytes: b"abc".to_vec(),
            },
            TtyChunk {
                chunk_seq: 2,
                bytes: b"def".to_vec(),
            },
            // Gap: 2 -> 5 should flip lagged and emit a resync intent.
            TtyChunk {
                chunk_seq: 5,
                bytes: b"ghi".to_vec(),
            },
        ]);
        let mut sink = RecordingControlSink::new();
        pump_terminal(&mut session, &mut source, &mut sink);

        // Bytes were fed in order despite the gap.
        assert!(session.screen().contents().contains("abcdefghi"));
        assert!(session.is_lagged());
        assert_eq!(sink.sent, vec![AgentControl::Resync]);
        assert!(source.is_drained());
    }

    // ── New Session launch (`n` on the Agents tab) ─────────────────────────

    #[test]
    fn n_on_agents_launches_a_session_through_the_launcher() {
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        let mut launcher = RecordingSessionLauncher::new();
        assert_eq!(
            handle_key_with_session(&mut app, key(KeyCode::Char('n')), &mut sink, &mut launcher),
            Flow::Continue
        );

        // The launcher was asked to create a session on the in-scope repo.
        assert_eq!(launcher.requested, vec!["core/web".to_string()]);
        // The launch advanced to attached and recorded the returned run/branch.
        let launch = app.session_launch.as_ref().expect("launch recorded");
        assert_eq!(launch.phase, SessionLaunchPhase::Attached);
        assert_eq!(launch.repo_id, "core/web");
        assert_eq!(launch.run_id.as_deref(), Some("agent_run.session-1"));
        assert_eq!(launch.branch.as_deref(), Some("agent/session-1"));
        // The new run's live terminal is mounted and attached.
        let term = app.terminal.as_ref().expect("terminal mounted");
        assert!(term.is_attached());
        assert_eq!(term.run_id(), "agent_run.session-1");
        // No terminal-control bytes were emitted by the launch itself.
        assert!(sink.sent.is_empty());
    }

    #[test]
    fn n_does_not_launch_when_no_repo_is_in_scope() {
        let mut app = App::new_render_only(jeryu_readmodel::TuiReadModel::default());
        app.set_tab(ActiveTab::Agents);
        let mut sink = RecordingControlSink::new();
        let mut launcher = RecordingSessionLauncher::new();
        handle_key_with_session(&mut app, key(KeyCode::Char('n')), &mut sink, &mut launcher);
        assert!(launcher.requested.is_empty());
        assert!(app.session_launch.is_none());
        assert!(app.terminal.is_none());
    }

    #[test]
    fn n_off_the_agents_tab_falls_through_to_normal_routing() {
        let mut app = App::new_render_only(sample_read_model());
        app.set_tab(ActiveTab::Mission);
        let mut sink = RecordingControlSink::new();
        let mut launcher = RecordingSessionLauncher::new();
        // Off the Agents tab `n` is an ordinary key — no launch, no panic.
        assert_eq!(
            handle_key_with_session(&mut app, key(KeyCode::Char('n')), &mut sink, &mut launcher),
            Flow::Continue
        );
        assert!(launcher.requested.is_empty());
        assert!(app.session_launch.is_none());
        assert_eq!(app.active_tab, ActiveTab::Mission);
    }

    #[test]
    fn session_router_preserves_existing_keys() {
        // Digit tab-select and quit still work through the session router, so
        // the affordance is purely additive to the keyboard model.
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        let mut launcher = RecordingSessionLauncher::new();
        handle_key_with_session(&mut app, key(KeyCode::Char('1')), &mut sink, &mut launcher);
        assert_eq!(app.active_tab, ActiveTab::Mission);
        assert!(launcher.requested.is_empty());
        assert_eq!(
            handle_key_with_session(&mut app, key(KeyCode::Char('q')), &mut sink, &mut launcher),
            Flow::Quit
        );
    }

    #[test]
    fn n_while_attached_routes_into_the_terminal_not_a_new_launch() {
        let mut app = agents_app();
        let mut sink = RecordingControlSink::new();
        let mut launcher = RecordingSessionLauncher::new();
        // Open + attach a terminal first (Enter), then press `n`.
        handle_key_with_session(&mut app, key(KeyCode::Enter), &mut sink, &mut launcher);
        sink.sent.clear();
        handle_key_with_session(&mut app, key(KeyCode::Char('n')), &mut sink, &mut launcher);
        // No new session launched; `n` became terminal input.
        assert!(launcher.requested.is_empty());
        assert!(app.session_launch.is_none());
        assert_eq!(sink.sent, vec![AgentControl::Input(b"n".to_vec())]);
    }
}
