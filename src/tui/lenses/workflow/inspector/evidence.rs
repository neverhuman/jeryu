use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::chrome::{draw_placeholder, empty_block};
use crate::tui::{lenses::workflow::model::WorkflowNode, theme::Theme};

pub(super) fn draw_evidence(f: &mut Frame, area: Rect, node: Option<&WorkflowNode>, theme: &Theme) {
    let Some(node) = node else {
        return draw_placeholder(f, area, "no node selected", theme);
    };
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "  Evidence capsule:",
        theme.bold(theme.text_primary),
    )));
    lines.push(Line::from(Span::styled(
        "    (stub - capsule_id wiring lands with the agent-review work)",
        theme.muted(),
    )));
    lines.push(Line::from(""));
    if let Some(bk) = &node.backend {
        lines.push(Line::from(Span::styled(
            format!("  Backend:    {:?}", bk),
            theme.secondary(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  Backend:    (none)",
            theme.muted(),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).block(empty_block(theme, " Evidence ")),
        area,
    );
}
