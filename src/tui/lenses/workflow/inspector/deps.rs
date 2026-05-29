use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::chrome::{draw_placeholder, empty_block};
use crate::tui::{lenses::workflow::model::*, theme::Theme};

pub(super) fn draw_deps(
    f: &mut Frame,
    area: Rect,
    snap: &WorkflowSnapshot,
    node: Option<&WorkflowNode>,
    theme: &Theme,
) {
    let Some(node) = node else {
        return draw_placeholder(f, area, "no node selected", theme);
    };

    let children: Vec<&WorkflowNode> = snap
        .nodes
        .iter()
        .filter(|n| n.deps.iter().any(|d| d == &node.id))
        .collect();

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "  Incoming:",
        theme.bold(theme.text_primary),
    )));
    if node.deps.is_empty() {
        lines.push(Line::from(Span::styled("    (none)", theme.muted())));
    } else {
        for dep in &node.deps {
            let dep_node = snap.node(dep);
            let label = dep_node.map(|n| n.label.as_str()).unwrap_or(dep.as_str());
            let glyph = dep_node.map(|n| n.status.glyph()).unwrap_or("?");
            lines.push(Line::from(Span::styled(
                format!("    {} {}", glyph, label),
                theme.secondary(),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Outgoing:",
        theme.bold(theme.text_primary),
    )));
    if children.is_empty() {
        lines.push(Line::from(Span::styled("    (none)", theme.muted())));
    } else {
        for c in &children {
            lines.push(Line::from(Span::styled(
                format!("    {} {}", c.status.glyph(), c.label),
                theme.secondary(),
            )));
        }
    }

    f.render_widget(
        Paragraph::new(lines).block(empty_block(theme, " Dependencies ")),
        area,
    );
}
