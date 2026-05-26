//! Owner: Interactive TUI subsystem — cursor-aware log viewer widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::log_viewer`.
//! Invariants: Pure rendering. Lines come pre-sliced from the caller (the
//! widget never owns the full log buffer). Gap markers are explicit so a
//! lost cursor range renders as a clear discontinuity, not a missing line.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme::palette::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Trace => "TRC",
            Self::Debug => "DBG",
            Self::Info => "INF",
            Self::Warn => "WRN",
            Self::Error => "ERR",
        }
    }
    fn color(self, p: &Palette) -> Color {
        match self {
            Self::Trace => p.stale,
            Self::Debug => p.cache,
            Self::Info => p.running,
            Self::Warn => p.warn,
            Self::Error => p.crit,
        }
    }
}

/// A single rendered log entry. `gap_before` indicates the cursor lost some
/// lines before this entry — the renderer draws a discontinuity marker.
#[derive(Debug, Clone, Copy)]
pub struct LogLine<'a> {
    pub timestamp: &'a str,
    pub level: LogLevel,
    pub source: &'a str,
    pub message: &'a str,
    pub gap_before: bool,
}

pub struct LogViewerProps<'a> {
    pub title: &'a str,
    pub lines: &'a [LogLine<'a>],
    /// Selected line index within `lines` (None = no selection).
    pub selected: Option<usize>,
    /// Whether the cursor is at the tail (auto-follow new lines).
    pub follow_tail: bool,
}

pub fn render(f: &mut Frame, props: &LogViewerProps, area: Rect, palette: &Palette) {
    let title = if props.follow_tail {
        format!(" {} [follow] ", props.title)
    } else {
        format!(" {} ", props.title)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.stale));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 8 || inner.height < 1 {
        return;
    }

    let max = inner.height as usize;
    let mut out: Vec<Line> = Vec::with_capacity(max);
    for (idx, ln) in props.lines.iter().take(max).enumerate() {
        if ln.gap_before {
            out.push(Line::from(Span::styled(
                " ── gap (events lost) ──",
                Style::default()
                    .fg(palette.warn)
                    .add_modifier(Modifier::DIM | Modifier::ITALIC),
            )));
        }
        let selected = props.selected == Some(idx);
        let base = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let spans = vec![
            Span::styled(
                format!(" {} ", ln.timestamp),
                base.fg(palette.stale),
            ),
            Span::styled(
                format!("{} ", ln.level.label()),
                base.fg(ln.level.color(palette))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<12} ", truncate(ln.source, 12)),
                base.fg(palette.cache),
            ),
            Span::styled(
                truncate(
                    ln.message,
                    inner.width.saturating_sub(32) as usize,
                ),
                base.fg(palette.running),
            ),
        ];
        out.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(out), inner);
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

    fn lines() -> [LogLine<'static>; 5] {
        [
            LogLine {
                timestamp: "12:00:01",
                level: LogLevel::Info,
                source: "runner-7",
                message: "Job 14445 started",
                gap_before: false,
            },
            LogLine {
                timestamp: "12:00:02",
                level: LogLevel::Debug,
                source: "runner-7",
                message: "Cloning repo",
                gap_before: false,
            },
            LogLine {
                timestamp: "12:01:00",
                level: LogLevel::Warn,
                source: "runner-7",
                message: "Cache miss",
                gap_before: true,
            },
            LogLine {
                timestamp: "12:01:02",
                level: LogLevel::Error,
                source: "runner-7",
                message: "Compile failed",
                gap_before: false,
            },
            LogLine {
                timestamp: "12:01:03",
                level: LogLevel::Trace,
                source: "runner-7",
                message: "Retrying",
                gap_before: false,
            },
        ]
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let l = lines();
        let props = LogViewerProps {
            title: "Logs",
            lines: &l,
            selected: Some(1),
            follow_tail: true,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 80, 12), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let l = lines();
        let props = LogViewerProps {
            title: "Logs",
            lines: &l,
            selected: None,
            follow_tail: false,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 120, 18), &p))
            .unwrap();
    }

    #[test]
    fn log_level_labels_are_three_chars() {
        for lvl in [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ] {
            assert_eq!(lvl.label().len(), 3);
        }
    }
}
