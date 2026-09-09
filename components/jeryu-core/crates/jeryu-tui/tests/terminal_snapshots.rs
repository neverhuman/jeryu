//! tuiwright-style snapshot tests for the live agent-terminal pane.
//!
//! These mirror `lens_snapshots.rs`: build an `App`, open + attach a terminal
//! session on the Agents tab, feed it scripted bytes, render one deterministic
//! frame through the full chrome (`render_once`), and assert the flattened cell
//! text shows the streamed bytes verbatim. No backend, no Docker, no real agent.

use jeryu_readmodel::sample_read_model;
use jeryu_tui::lenses::agents::AgentTerminalSession;
use jeryu_tui::{ActiveTab, App, StreamMode, render_once};

/// Build an Agents-tab app with a terminal session that has been fed `bytes`.
/// `attached` controls whether the pane owns the lower body.
fn agents_app_with_terminal(bytes: &[u8], attached: bool, lagged: bool) -> App {
    let mut app = App::new_render_only(sample_read_model());
    app.set_tab(ActiveTab::Agents);
    let mut session = AgentTerminalSession::new("agent_run.snap", 40, 120);
    session.feed(bytes);
    if attached {
        session.attach();
    }
    session.set_lagged(lagged);
    app.terminal = Some(session);
    app
}

const SCRIPT: &[u8] = b"$ cargo test\r\nrunning 3 tests\r\n";

#[test]
fn attached_terminal_renders_scripted_bytes_at_120x40() {
    let app = agents_app_with_terminal(SCRIPT, true, false);
    let ink = render_once(&app, 120, 40, StreamMode::Live);
    assert!(ink.contains("cargo test"), "command line not painted");
    assert!(ink.contains("running 3 tests"), "test output not painted");
    // The pane chrome carries the run id and the attached posture.
    assert!(ink.contains("agent_run.snap"), "terminal title missing");
    assert!(ink.contains("ATTACHED"), "attached status missing");
    // Chrome still composes the Flight Deck header.
    assert!(ink.contains("LIVE"), "stream badge missing");
}

#[test]
fn attached_terminal_renders_scripted_bytes_at_80x24() {
    let app = agents_app_with_terminal(SCRIPT, true, false);
    let ink = render_once(&app, 80, 24, StreamMode::Live);
    assert!(ink.contains("cargo test"));
    assert!(ink.contains("running 3 tests"));
    assert!(ink.contains("ATTACHED"));
}

#[test]
fn focused_input_pane_shows_typed_prompt_and_attached_footer() {
    // A focused (attached) terminal echoes the agent's prompt verbatim and marks
    // the pane as ATTACHED so the operator knows keystrokes are being routed.
    let app = agents_app_with_terminal(b"agent@box:~$ ls -la\r\n", true, false);
    let ink = render_once(&app, 120, 40, StreamMode::Live);
    assert!(ink.contains("agent@box:~$ ls -la"));
    assert!(ink.contains("ATTACHED"));
    assert!(ink.contains("Ctrl-] detach"));
}

#[test]
fn lagged_terminal_surfaces_resync_banner() {
    let app = agents_app_with_terminal(SCRIPT, true, true);
    let ink = render_once(&app, 120, 40, StreamMode::Live);
    assert!(ink.contains("cargo test"));
    assert!(ink.contains("lagged"), "lagged banner missing");
}

#[test]
fn lagged_terminal_surfaces_resync_banner_at_80x24() {
    let app = agents_app_with_terminal(SCRIPT, true, true);
    let ink = render_once(&app, 80, 24, StreamMode::Live);
    assert!(ink.contains("lagged"));
}

#[test]
fn detached_session_keeps_the_lifecycle_table() {
    // A present-but-detached session must not steal the lower body: the Agents
    // lens keeps painting its lifecycle table (the sample fleet rows).
    let app = agents_app_with_terminal(SCRIPT, false, false);
    let ink = render_once(&app, 120, 40, StreamMode::Live);
    assert!(ink.contains("Lifecycle"), "lifecycle table replaced");
    assert!(ink.contains("agent-wrath-17"), "fleet rows missing");
    assert!(
        !ink.contains("running 3 tests"),
        "terminal painted while detached"
    );
}
