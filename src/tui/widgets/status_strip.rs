//! Owner: Interactive TUI subsystem — bottom status strip widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::status_strip`.
//! Invariants: Pure rendering — key hints + frame metrics. No I/O.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme::palette::Palette;

/// A `(key, description)` keybinding hint pair.
#[derive(Debug, Clone, Copy)]
pub struct KeyHint<'a> {
    pub key: &'a str,
    pub desc: &'a str,
}

/// Frame timing metrics displayed on the right side of the strip.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameMetrics {
    /// Last frame render duration in milliseconds.
    pub frame_ms: u32,
    /// Active frames-per-second target.
    pub fps: u16,
    /// Number of dropped frames in the last sample window.
    pub dropped: u16,
}

pub struct StatusStripProps<'a> {
    pub hints: &'a [KeyHint<'a>],
    pub metrics: FrameMetrics,
}

pub fn render(f: &mut Frame, props: &StatusStripProps, area: Rect, palette: &Palette) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(28)])
        .split(inner);

    // Key hints
    let mut spans: Vec<Span> = Vec::new();
    for (idx, hint) in props.hints.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(
            format!("{}", hint.key),
            Style::default()
                .fg(palette.running)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}", hint.desc),
            Style::default().fg(palette.stale),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), cols[0]);

    // Frame metrics
    let metrics = format!(
        " {fps:>3}fps  {ms:>3}ms  drop:{drop:>3}",
        fps = props.metrics.fps,
        ms = props.metrics.frame_ms,
        drop = props.metrics.dropped
    );
    let metrics_color = if props.metrics.dropped > 0 {
        palette.warn
    } else {
        palette.stale
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            metrics,
            Style::default().fg(metrics_color),
        ))),
        cols[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample_hints() -> [KeyHint<'static>; 3] {
        [
            KeyHint {
                key: "?",
                desc: "help",
            },
            KeyHint {
                key: ":",
                desc: "cmd",
            },
            KeyHint {
                key: "Esc",
                desc: "back",
            },
        ]
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let hints = sample_hints();
        let props = StatusStripProps {
            hints: &hints,
            metrics: FrameMetrics {
                frame_ms: 14,
                fps: 60,
                dropped: 0,
            },
        };
        term.draw(|f| render(f, &props, Rect::new(0, 22, 80, 2), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let hints = sample_hints();
        let props = StatusStripProps {
            hints: &hints,
            metrics: FrameMetrics {
                frame_ms: 8,
                fps: 120,
                dropped: 2,
            },
        };
        term.draw(|f| render(f, &props, Rect::new(0, 34, 120, 2), &p))
            .unwrap();
    }
}
