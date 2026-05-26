use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::chrome::empty_block;
use crate::tui::{app::LiveLogState, lenses::workflow::model::WorkflowNode, theme::Theme};

pub(super) fn draw_logs(
    f: &mut Frame,
    area: Rect,
    node: Option<&WorkflowNode>,
    live_log: &LiveLogState,
    theme: &Theme,
) {
    let mut lines = Vec::new();
    let header = match node {
        Some(n) => format!("  tail for {}", n.id),
        None => "  no node selected".into(),
    };
    lines.push(Line::from(Span::styled(header, theme.muted())));
    lines.push(Line::from(""));

    if let Some(err) = &live_log.error {
        lines.push(Line::from(Span::styled(
            format!("  log error: {}", err),
            theme.bold(theme.fail),
        )));
    } else if live_log.text.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no logs yet - live tail will appear here)",
            theme.muted(),
        )));
    } else {
        let max_rows = area.height.saturating_sub(4) as usize;
        let log_lines: Vec<&str> = live_log.text.lines().collect();
        let start = log_lines.len().saturating_sub(max_rows);
        for line in &log_lines[start..] {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                theme.primary(),
            )));
        }
    }

    f.render_widget(
        Paragraph::new(lines).block(empty_block(theme, " Logs (live) ")),
        area,
    );
}
