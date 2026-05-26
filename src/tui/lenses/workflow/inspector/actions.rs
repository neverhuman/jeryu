use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::chrome::{draw_placeholder, empty_block};
use crate::tui::{lenses::workflow::model::*, theme::Theme};

pub(super) fn draw_actions(
    f: &mut Frame,
    area: Rect,
    node: Option<&WorkflowNode>,
    action_message: Option<&str>,
    theme: &Theme,
) {
    let Some(node) = node else {
        return draw_placeholder(f, area, "no node selected", theme);
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "  Available actions:",
            theme.bold(theme.text_primary),
        )),
        Line::from(""),
    ];

    let mut add = |label: &str, hint: &str, color: ratatui::style::Color| {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  [ {} ]", label),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(hint.to_string(), theme.muted()),
        ]));
    };

    add(
        " Rerun       ",
        "press R (stub until backend wiring)",
        theme.running,
    );
    if node.kind.is_rollback_eligible() {
        add(
            " Rollback    ",
            "press r - builds rollback report (dry-run)",
            theme.warning,
        );
    }
    if matches!(node.kind, WorkflowNodeKind::AgentReview { .. }) {
        add(
            " View prompt",
            "stub: agent review wiring pending",
            theme.agent,
        );
    }
    if node.backend.is_some() {
        add(
            " Open in GitLab",
            "stub: open backend job page",
            theme.production,
        );
    }
    add(
        " View capsule",
        "stub: capsule evidence viewer",
        theme.vti_fire,
    );

    if let Some(msg) = action_message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Last action:",
            theme.bold(theme.text_primary),
        )));
        lines.push(Line::from(Span::styled(
            format!("    {}", msg),
            theme.bold(theme.warning),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).block(empty_block(theme, " Actions ")),
        area,
    );
}
