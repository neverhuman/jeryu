//! Owner: Interactive TUI subsystem — `:` command palette overlay (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::command_palette`.
//! Invariants: Pure rendering. The palette is a centered overlay with a
//! prompt + input string + filtered candidate list. Filter is supplied by
//! the caller (this widget does not own search state).

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::tui::theme::palette::Palette;

/// A candidate action entry in the palette.
#[derive(Debug, Clone, Copy)]
pub struct PaletteEntry<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub risk: &'a str,
}

pub struct CommandPaletteProps<'a> {
    pub query: &'a str,
    pub cursor: usize,
    pub entries: &'a [PaletteEntry<'a>],
    pub selected: usize,
    pub footer: &'a str,
}

pub fn render(f: &mut Frame, props: &CommandPaletteProps, area: Rect, palette: &Palette) {
    // Center an overlay roughly 60×16, capped to area.
    let w = area.width.min(80);
    let h = area.height.min(20);
    if w < 20 || h < 6 {
        return;
    }
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);
    f.render_widget(Clear, overlay);

    let block = Block::default()
        .title(" : command palette ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.warn));
    let inner = block.inner(overlay);
    f.render_widget(block, overlay);

    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // input
            Constraint::Min(1),    // entries
            Constraint::Length(1), // footer
        ])
        .split(inner);

    // Input row with cursor.
    let cursor_indicator = if props.cursor <= props.query.chars().count() {
        let pre: String = props.query.chars().take(props.cursor).collect();
        let post: String = props.query.chars().skip(props.cursor).collect();
        Line::from(vec![
            Span::styled(": ", Style::default().fg(palette.warn)),
            Span::styled(pre, Style::default().fg(palette.running)),
            Span::styled(
                "▏",
                Style::default()
                    .fg(palette.warn)
                    .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK),
            ),
            Span::styled(post, Style::default().fg(palette.running)),
        ])
    } else {
        Line::from(vec![
            Span::styled(": ", Style::default().fg(palette.warn)),
            Span::styled(
                props.query.to_string(),
                Style::default().fg(palette.running),
            ),
        ])
    };
    f.render_widget(Paragraph::new(cursor_indicator), sections[0]);

    // Filtered entries.
    let max = sections[1].height as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(max);
    for (i, e) in props.entries.iter().take(max).enumerate() {
        let active = i == props.selected;
        let row_style = if active {
            Style::default()
                .fg(palette.ok)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(palette.running)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<4} ", e.risk),
                Style::default().fg(palette.warn),
            ),
            Span::styled(format!("{:<28} ", truncate(e.label, 28)), row_style),
            Span::styled(format!("{}", e.id), Style::default().fg(palette.stale)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), sections[1]);

    // Footer.
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {} ", props.footer),
            Style::default()
                .fg(palette.stale)
                .add_modifier(Modifier::DIM),
        ))),
        sections[2],
    );
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else if max > 1 {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    } else {
        s.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn entries() -> [PaletteEntry<'static>; 4] {
        [
            PaletteEntry {
                id: "job.retry",
                label: "Retry failed job",
                risk: "R1",
            },
            PaletteEntry {
                id: "release.canary",
                label: "Start canary rollout",
                risk: "R4",
            },
            PaletteEntry {
                id: "runner.drain",
                label: "Drain runner pool",
                risk: "R3",
            },
            PaletteEntry {
                id: "agent.kill",
                label: "Kill agent session",
                risk: "R5",
            },
        ]
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let e = entries();
        let props = CommandPaletteProps {
            query: "ret",
            cursor: 3,
            entries: &e,
            selected: 0,
            footer: "↑↓ select  Enter run  Esc close",
        };
        term.draw(|f| render(f, &props, f.area(), &p)).unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let e = entries();
        let props = CommandPaletteProps {
            query: "",
            cursor: 0,
            entries: &e,
            selected: 2,
            footer: "↑↓ select  Enter run  Esc close",
        };
        term.draw(|f| render(f, &props, f.area(), &p)).unwrap();
    }
}
