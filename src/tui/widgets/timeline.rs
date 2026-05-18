//! Owner: Interactive TUI subsystem — event timeline widget
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::timeline`
//! Invariants: Timeline renders from `TuiEvent` slice; severity filtering is pure.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::api::entity::Severity;
use crate::api::events::TuiEvent;
use crate::tui::theme::Theme;

/// Render the bottom event timeline from recent events.
pub fn render_timeline(
    f: &mut Frame,
    area: Rect,
    events: &[&TuiEvent],
    min_severity: Severity,
    theme: &Theme,
) {
    let block = Block::default()
        .title(" [ Events ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if events.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  Waiting for events...", theme.muted())),
            inner,
        );
        return;
    }

    let max_lines = inner.height as usize;
    let lines: Vec<Line> = events
        .iter()
        .filter(|e| e.severity <= min_severity)
        .take(max_lines)
        .map(|event| {
            let ts = event.timestamp.format("%H:%M:%S").to_string();
            let sev_color = theme.severity_color(event.severity);
            let kind_label = event.kind.label();

            Line::from(vec![
                Span::styled(format!(" {} ", ts), theme.muted()),
                Span::styled(
                    format!("{} ", event.severity.label()),
                    Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<28} ", kind_label), theme.secondary()),
                Span::styled(
                    truncate_summary(&event.summary, inner.width.saturating_sub(48) as usize),
                    theme.primary(),
                ),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn truncate_summary(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() <= max {
        s.to_string()
    } else if max > 3 {
        format!("{}...", &s[..max - 3])
    } else {
        s[..max].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_works() {
        assert_eq!(truncate_summary("hello", 10), "hello");
        assert_eq!(truncate_summary("hello world!", 8), "hello...");
    }
}
