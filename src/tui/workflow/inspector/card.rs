//! Owner: Interactive TUI subsystem — Delivery inspector selected-node detail cards
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Render-only; reads the selected node and the workflow snapshot,
//! never mutates them.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::super::model::*;
use super::{draw_placeholder, empty_block, row};
use crate::tui::theme::Theme;

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

    // Outgoing children: nodes whose deps contain this node id.
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
        "    (stub — capsule_id wiring lands with the agent-review work)",
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
