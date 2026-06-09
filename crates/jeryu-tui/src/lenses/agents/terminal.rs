//! Live agent-terminal session: a vt100 screen emulator fed by decoded
//! `AgentTtyEvent` bytes.
//!
//! Invariants: pure in-memory emulation. [`AgentTerminalSession`] drives a
//! [`vt100::Parser`] from raw terminal bytes so the Agents lens can paint the
//! agent's screen grid verbatim. No I/O lives here — the production
//! [`WsTtyBridge`](crate::runtime::tty) feeds decoded bytes in behind the
//! [`TtySource`](crate::runtime::tty::TtySource) seam.
//!
//! `vt100::Parser` is neither `Clone` nor `Debug`, so both are implemented by
//! hand: [`Clone`] reconstructs an equivalent parser from the source screen's
//! `state_formatted()` escape stream (the visible grid + attributes), keeping
//! [`App`](crate::app::App) `Clone` without retaining the full byte history.

/// A single attached/attachable agent terminal. Holds the emulator screen grid
/// plus the attach/lag posture surfaced in the status line.
pub struct AgentTerminalSession {
    run_id: String,
    parser: vt100::Parser,
    attached: bool,
    lagged: bool,
    last_chunk_seq: u64,
    rows: u16,
    cols: u16,
}

impl AgentTerminalSession {
    /// Open a detached session of the given grid size for `run_id`.
    pub fn new(run_id: impl Into<String>, rows: u16, cols: u16) -> Self {
        Self {
            run_id: run_id.into(),
            parser: vt100::Parser::new(rows, cols, 0),
            attached: false,
            lagged: false,
            last_chunk_seq: 0,
            rows,
            cols,
        }
    }

    /// The agent run this session mirrors (scope `agent_run.{id}`).
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Drive the emulator with a chunk of decoded terminal bytes.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    /// Borrow the current screen grid for rendering.
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Resize the emulator grid (driven by the pane geometry).
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
        self.parser.screen_mut().set_size(rows, cols);
    }

    /// Current grid size as `(rows, cols)`.
    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Mark/clear the lagged posture (a detected `chunk_seq` gap).
    pub fn set_lagged(&mut self, lagged: bool) {
        self.lagged = lagged;
    }

    /// Whether a stream gap has been observed since the last resync.
    pub fn is_lagged(&self) -> bool {
        self.lagged
    }

    /// Attach: keys now route into the terminal instead of the lens chrome.
    pub fn attach(&mut self) {
        self.attached = true;
    }

    /// Detach: keys fall back to the normal Flight Deck routing.
    pub fn detach(&mut self) {
        self.attached = false;
    }

    /// Whether this session owns keyboard input.
    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// Record the last chunk sequence and flag a gap as lagged. Returns `true`
    /// when a gap was detected (the caller emits a resync intent).
    pub fn observe_chunk_seq(&mut self, chunk_seq: u64) -> bool {
        let gap = chunk_seq > self.last_chunk_seq + 1 && self.last_chunk_seq != 0;
        self.last_chunk_seq = chunk_seq;
        if gap {
            self.lagged = true;
        }
        gap
    }

    /// The last chunk sequence consumed from the source.
    pub fn last_chunk_seq(&self) -> u64 {
        self.last_chunk_seq
    }
}

impl Clone for AgentTerminalSession {
    fn clone(&self) -> Self {
        // `vt100::Parser` is not `Clone`; rebuild an equivalent emulator from
        // the source screen's formatted state (visible grid + attributes).
        let mut parser = vt100::Parser::new(self.rows, self.cols, 0);
        parser.process(&self.parser.screen().state_formatted());
        Self {
            run_id: self.run_id.clone(),
            parser,
            attached: self.attached,
            lagged: self.lagged,
            last_chunk_seq: self.last_chunk_seq,
            rows: self.rows,
            cols: self.cols,
        }
    }
}

impl std::fmt::Debug for AgentTerminalSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentTerminalSession")
            .field("run_id", &self.run_id)
            .field("attached", &self.attached)
            .field("lagged", &self.lagged)
            .field("last_chunk_seq", &self.last_chunk_seq)
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_renders_plain_text_into_the_screen() {
        let mut session = AgentTerminalSession::new("agent_run.42", 24, 80);
        session.feed(b"$ cargo test\r\nrunning 3 tests\r\n");
        let dump = session.screen().contents();
        assert!(dump.contains("cargo test"));
        assert!(dump.contains("running 3 tests"));
    }

    #[test]
    fn feed_applies_ansi_color_attributes() {
        let mut session = AgentTerminalSession::new("agent_run.7", 24, 80);
        // Red "FAIL", reset, then plain text.
        session.feed(b"\x1b[31mFAIL\x1b[0m ok\r\n");
        let screen = session.screen();
        assert_eq!(screen.cell(0, 0).unwrap().contents(), "F");
        assert_eq!(screen.cell(0, 0).unwrap().fgcolor(), vt100::Color::Idx(1));
        // After the reset the foreground returns to default.
        assert_eq!(screen.cell(0, 5).unwrap().fgcolor(), vt100::Color::Default);
    }

    #[test]
    fn resize_updates_the_parser_grid() {
        let mut session = AgentTerminalSession::new("agent_run.1", 24, 80);
        assert_eq!(session.screen().size(), (24, 80));
        session.resize(40, 120);
        assert_eq!(session.size(), (40, 120));
        assert_eq!(session.screen().size(), (40, 120));
    }

    #[test]
    fn attach_detach_and_lagged_toggle() {
        let mut session = AgentTerminalSession::new("agent_run.1", 24, 80);
        assert!(!session.is_attached());
        session.attach();
        assert!(session.is_attached());
        session.detach();
        assert!(!session.is_attached());

        assert!(!session.is_lagged());
        session.set_lagged(true);
        assert!(session.is_lagged());
        session.set_lagged(false);
        assert!(!session.is_lagged());
    }

    #[test]
    fn chunk_seq_gap_flags_lagged() {
        let mut session = AgentTerminalSession::new("agent_run.1", 24, 80);
        assert!(!session.observe_chunk_seq(1));
        assert!(!session.observe_chunk_seq(2));
        // A jump from 2 to 5 is a gap.
        assert!(session.observe_chunk_seq(5));
        assert!(session.is_lagged());
        assert_eq!(session.last_chunk_seq(), 5);
    }

    #[test]
    fn clone_reconstructs_the_visible_screen() {
        let mut session = AgentTerminalSession::new("agent_run.9", 24, 80);
        session.feed(b"\x1b[32mPASS\x1b[0m build\r\n");
        session.attach();
        session.set_lagged(true);
        let cloned = session.clone();
        assert!(cloned.screen().contents().contains("PASS build"));
        assert_eq!(
            cloned.screen().cell(0, 0).unwrap().fgcolor(),
            vt100::Color::Idx(2)
        );
        assert!(cloned.is_attached());
        assert!(cloned.is_lagged());
        assert_eq!(cloned.run_id(), "agent_run.9");
    }
}
