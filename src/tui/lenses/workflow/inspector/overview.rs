use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::chrome::{draw_placeholder, empty_block, row};
use crate::tui::{
    lenses::workflow::model::{WorkflowNode, WorkflowStatus},
    theme::Theme,
};

pub(super) fn draw_overview(f: &mut Frame, area: Rect, node: Option<&WorkflowNode>, theme: &Theme) {
    let Some(node) = node else {
        return draw_placeholder(f, area, "no node selected", theme);
    };
    let status_color = match node.status {
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Skipped => theme.skipped,
        _ => theme.waiting,
    };

    let mut lines = Vec::new();
    lines.push(row(
        "Status",
        node.status.label(),
        theme.bold(status_color),
        theme,
    ));
    lines.push(row("Kind", node.kind.label(), theme.secondary(), theme));
    if let Some(cmd) = &node.command {
        lines.push(row("Command", cmd, theme.primary(), theme));
    }
    if let Some(pct) = node.progress_pct {
        lines.push(row(
            "Progress",
            &format!("{}%", pct),
            theme.bold(status_color),
            theme,
        ));
    }
    if let Some(eta) = node.eta_secs {
        lines.push(row("ETA", &format!("{}s", eta), theme.secondary(), theme));
    }
    if let Some(dur) = node.duration_secs {
        lines.push(row(
            "Duration",
            &format!("{:.1}s", dur),
            theme.secondary(),
            theme,
        ));
    }
    if let Some(v) = node.vti_status.as_ref() {
        lines.push(row("VTI", v.badge(), theme.bold(theme.vti_fire), theme));
    }
    if let Some(c) = node.cache_verdict.as_ref() {
        lines.push(row("Cache", c.badge(), theme.bold(theme.ok), theme));
    }
    if node.critical_path {
        lines.push(Line::from(Span::styled(
            "  [CRITICAL PATH]",
            theme.bold(theme.fail),
        )));
    }
    if let Some(reason) = &node.reason {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  Reason:", theme.muted())));
        lines.push(Line::from(Span::styled(
            format!("    {}", reason),
            theme.secondary(),
        )));
    }
    if !node.tags.is_empty() {
        lines.push(Line::from(""));
        lines.push(row("Tags", &node.tags.join(", "), theme.secondary(), theme));
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(empty_block(theme, " Overview ")),
        area,
    );
}
