//! Owner: Interactive TUI subsystem — scrolling event tape widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::event_tape`.
//! Invariants: Pure rendering. The "tape" is a single horizontal strip of
//! recent event glyphs, oldest-left → newest-right, with the newest event
//! visually highlighted. Used by Mission lens.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme::palette::Palette;

/// Event tape cell.
#[derive(Debug, Clone, Copy)]
pub struct TapeCell<'a> {
    pub glyph: &'a str,
    pub severity: TapeSeverity,
    pub label: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapeSeverity {
    Info,
    Notable,
    Warning,
    Error,
    Critical,
}

impl TapeSeverity {
    fn color(self, p: &Palette) -> Color {
        match self {
            Self::Info => p.stale,
            Self::Notable => p.running,
            Self::Warning => p.warn,
            Self::Error => p.queued,
            Self::Critical => p.crit,
        }
    }
}

pub struct EventTapeProps<'a> {
    pub title: &'a str,
    pub cells: &'a [TapeCell<'a>],
    /// Highlight the rightmost (newest) cell.
    pub highlight_newest: bool,
}

pub fn render(f: &mut Frame, props: &EventTapeProps, area: Rect, palette: &Palette) {
    let block = Block::default()
        .title(format!(" {} ", props.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.stale));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 4 || inner.height < 1 {
        return;
    }

    // Strip line: glyphs with colors.
    let last_idx = props.cells.len().saturating_sub(1);
    let mut spans: Vec<Span> = Vec::with_capacity(props.cells.len() * 2);
    for (i, c) in props.cells.iter().enumerate() {
        let mut style = Style::default().fg(c.severity.color(palette));
        if props.highlight_newest && i == last_idx {
            style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
        }
        spans.push(Span::styled(c.glyph.to_string(), style));
        if i + 1 < props.cells.len() {
            spans.push(Span::raw(" "));
        }
    }
    let strip_area = Rect::new(inner.x, inner.y, inner.width, 1);
    f.render_widget(Paragraph::new(Line::from(spans)), strip_area);

    // Newest cell's label is rendered below.
    if inner.height >= 2
        && let Some(last) = props.cells.last()
    {
        let label_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        let style = Style::default()
            .fg(last.severity.color(palette))
            .add_modifier(Modifier::DIM);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  ↑ {}", last.label),
                style,
            ))),
            label_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn cells() -> [TapeCell<'static>; 6] {
        [
            TapeCell {
                glyph: "·",
                severity: TapeSeverity::Info,
                label: "tick",
            },
            TapeCell {
                glyph: "●",
                severity: TapeSeverity::Notable,
                label: "job started",
            },
            TapeCell {
                glyph: "!",
                severity: TapeSeverity::Warning,
                label: "retry",
            },
            TapeCell {
                glyph: "✗",
                severity: TapeSeverity::Error,
                label: "fail",
            },
            TapeCell {
                glyph: "⊘",
                severity: TapeSeverity::Critical,
                label: "blocked",
            },
            TapeCell {
                glyph: "✓",
                severity: TapeSeverity::Notable,
                label: "passed",
            },
        ]
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let c = cells();
        let props = EventTapeProps {
            title: "Tape",
            cells: &c,
            highlight_newest: true,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 80, 4), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let c = cells();
        let props = EventTapeProps {
            title: "Tape",
            cells: &c,
            highlight_newest: false,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 120, 4), &p))
            .unwrap();
    }
}
