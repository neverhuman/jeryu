//! Live agent-terminal pane widget.
//!
//! Invariants: pure render. Paints a [`vt100::Screen`] grid verbatim into the
//! ratatui [`Buffer`](ratatui::buffer::Buffer) for a given `Rect` so the
//! `render_once` cell-flattening surface shows the streamed bytes exactly as the
//! agent emitted them, plus a one-line status footer carrying the
//! attached/detached + lagged posture. No backend I/O.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::lenses::agents::terminal::AgentTerminalSession;

/// Render the agent terminal `session` into `area`: a bordered grid of the
/// emulator screen with a status footer.
pub fn render(f: &mut Frame, session: &AgentTerminalSession, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Terminal — {} ", session.run_id()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);
    let grid = rows[0];
    let status = rows[1];

    paint_grid(f, session, grid);
    paint_status(f, session, status);
}

/// Copy each emulator cell's glyph + attributes into the frame buffer, clamped
/// to the visible grid `area`.
fn paint_grid(f: &mut Frame, session: &AgentTerminalSession, area: Rect) {
    let screen = session.screen();
    let (screen_rows, screen_cols) = screen.size();
    let buf = f.buffer_mut();

    for dy in 0..area.height {
        if dy >= screen_rows {
            break;
        }
        for dx in 0..area.width {
            if dx >= screen_cols {
                break;
            }
            let Some(src) = screen.cell(dy, dx) else {
                continue;
            };
            if src.is_wide_continuation() {
                continue;
            }
            let Some(dest) = buf.cell_mut((area.x + dx, area.y + dy)) else {
                continue;
            };
            if src.has_contents() {
                dest.set_symbol(src.contents());
            } else {
                dest.set_char(' ');
            }
            dest.set_style(cell_style(src));
        }
    }
}

/// Translate a vt100 cell's colors + emphasis into a ratatui [`Style`].
fn cell_style(cell: &vt100::Cell) -> Style {
    let mut style = Style::default()
        .fg(map_color(cell.fgcolor()))
        .bg(map_color(cell.bgcolor()));
    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if cell.inverse() {
        style = style.add_modifier(Modifier::REVERSED);
    }
    style
}

/// Map a `vt100::Color` onto a ratatui [`Color`].
fn map_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Paint the one-line attach/lag status footer.
fn paint_status(f: &mut Frame, session: &AgentTerminalSession, area: Rect) {
    let (attach_text, attach_style) = if session.is_attached() {
        (
            "ATTACHED",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        ("detached", Style::default().fg(Color::Gray))
    };

    let mut spans = vec![
        Span::styled(attach_text, attach_style),
        Span::raw(" · Ctrl-] detach · Ctrl-C interrupt"),
    ];
    if session.is_lagged() {
        spans.push(Span::styled(
            " · ⚠ lagged (resyncing)",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, session: &AgentTerminalSession) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, session, f.area())).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn paints_screen_text_and_status() {
        let mut session = AgentTerminalSession::new("agent_run.42", 24, 80);
        session.feed(b"$ cargo test\r\nrunning 3 tests\r\n");
        session.attach();
        let out = ink(80, 24, &session);
        assert!(out.contains("cargo test"));
        assert!(out.contains("running 3 tests"));
        assert!(out.contains("ATTACHED"));
        assert!(out.contains("agent_run.42"));
    }

    #[test]
    fn lagged_status_is_surfaced() {
        let mut session = AgentTerminalSession::new("agent_run.9", 24, 80);
        session.feed(b"building...\r\n");
        session.attach();
        session.set_lagged(true);
        let out = ink(80, 24, &session);
        assert!(out.contains("lagged"));
    }
}
