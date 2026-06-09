//! Event-loop scaffold: keyboard routing for the Flight Deck.
//!
//! The real product drives this from a crossterm event stream; here the routing
//! is factored into a pure [`handle_key`] so it is unit-testable without a
//! terminal. The event loop itself ([`run_loop`]) is a thin wrapper a binary
//! would call; it is not exercised by the standalone test suite (no TTY).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{ActiveTab, App};
use crate::lenses::agents::AgentTerminalSession;
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
}
