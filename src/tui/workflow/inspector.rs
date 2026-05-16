//! Owner: Interactive TUI subsystem — Delivery inspector pane
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Render-only; reads app state, never mutates it.
//!
//! Right-side detail pane for the Delivery view. Five sub-tabs:
//!   * Overview  — status / kind / command / deps / timing / badges / reason
//!   * Logs      — live tail from LiveLogState (per-node)
//!   * Deps      — incoming + outgoing dependency lists
//!   * Evidence  — capsule id, artifacts, related PR labels (stub)
//!   * Actions   — context-sensitive buttons (Rerun, Open in GitLab,
//!                 View capsule, Rollback for promote nodes)
//!
//! When the terminal is too narrow for a side pane, the legacy modal
//! overlay in `ui.rs` is rendered instead.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::model::*;
use crate::tui::app::LiveLogState;
use crate::tui::theme::Theme;

/// Recommended width of the inspector pane in cols.
pub const INSPECTOR_W: u16 = 48;
/// Terminal width below which the pane collapses to a modal overlay.
pub const INSPECTOR_MIN_TERM_W: u16 = 140;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Overview,
    Logs,
    Deps,
    Evidence,
    Actions,
}

impl InspectorTab {
    pub const ALL: [InspectorTab; 5] = [
        Self::Overview,
        Self::Logs,
        Self::Deps,
        Self::Evidence,
        Self::Actions,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Logs => "Logs",
            Self::Deps => "Deps",
            Self::Evidence => "Evidence",
            Self::Actions => "Actions",
        }
    }

    pub fn next(self) -> Self {
        let mut idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        idx = (idx + 1) % Self::ALL.len();
        Self::ALL[idx]
    }

    pub fn prev(self) -> Self {
        let mut idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        idx = if idx == 0 { Self::ALL.len() - 1 } else { idx - 1 };
        Self::ALL[idx]
    }
}

/// Render the inspector pane in `area`. The selected node is drawn from
/// `pr.snapshot` using the (phase_idx, node_idx) cursor on `nav`.
pub fn draw_inspector_pane(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav_node_id: Option<&str>,
    tab: InspectorTab,
    live_log: &LiveLogState,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let Some(pr) = delivery.selected() else {
        f.render_widget(
            empty_block(theme, " Inspect "),
            area,
        );
        return;
    };
    let node = nav_node_id.and_then(|id| pr.snapshot.node(id));

    // Header (tab strip) + content split.
    let header_h: u16 = 3;
    let header_area = Rect::new(area.x, area.y, area.width, header_h.min(area.height));
    let content_area = Rect::new(
        area.x,
        area.y + header_h,
        area.width,
        area.height.saturating_sub(header_h),
    );

    draw_tab_strip(f, header_area, pr, node, tab, theme);

    if content_area.height == 0 {
        return;
    }

    match tab {
        InspectorTab::Overview => draw_overview(f, content_area, node, theme),
        InspectorTab::Logs => draw_logs(f, content_area, node, live_log, theme),
        InspectorTab::Deps => draw_deps(f, content_area, &pr.snapshot, node, theme),
        InspectorTab::Evidence => draw_evidence(f, content_area, node, theme),
        InspectorTab::Actions => draw_actions(f, content_area, node, theme),
    }
}

fn empty_block<'a>(theme: &Theme, title: &'a str) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle))
}

fn draw_tab_strip(
    f: &mut Frame,
    area: Rect,
    pr: &PullRequestView,
    node: Option<&WorkflowNode>,
    selected: InspectorTab,
    theme: &Theme,
) {
    let title_text = match node {
        Some(n) => format!(" {} {} ", n.status.glyph(), n.label.chars().take(28).collect::<String>()),
        None => format!(" PR #{} ", pr.number),
    };
    let block = Block::default()
        .title(title_text)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_accent));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans: Vec<Span> = Vec::new();
    for tab in InspectorTab::ALL {
        let style = if tab == selected {
            Style::default()
                .fg(theme.text_inverse)
                .bg(theme.border_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.muted()
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn draw_overview(f: &mut Frame, area: Rect, node: Option<&WorkflowNode>, theme: &Theme) {
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
    lines.push(row("Status", node.status.label(), theme.bold(status_color), theme));
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
        lines.push(Line::from(Span::styled(
            "  Reason:",
            theme.muted(),
        )));
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

fn draw_logs(
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
            "  (no logs yet — live tail will appear here)",
            theme.muted(),
        )));
    } else {
        // Show only the tail that fits in the pane height.
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

fn draw_deps(
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
    lines.push(Line::from(Span::styled("  Incoming:", theme.bold(theme.text_primary))));
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
    lines.push(Line::from(Span::styled("  Outgoing:", theme.bold(theme.text_primary))));
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

fn draw_evidence(f: &mut Frame, area: Rect, node: Option<&WorkflowNode>, theme: &Theme) {
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

fn draw_actions(f: &mut Frame, area: Rect, node: Option<&WorkflowNode>, theme: &Theme) {
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
            Span::styled(format!("  [ {} ]", label), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(hint.to_string(), theme.muted()),
        ]));
    };

    add(" Rerun       ", "press R (stub until backend wiring)", theme.running);
    if node.kind.is_rollback_eligible() {
        add(" Rollback    ", "press r — reverts environment (Wave 8)", theme.warning);
    }
    if matches!(node.kind, WorkflowNodeKind::AgentReview { .. }) {
        add(" View prompt", "stub: agent review wiring pending", theme.agent);
    }
    if let Some(bk) = &node.backend {
        let _ = bk;
        add(" Open in GitLab", "stub: open backend job page", theme.production);
    }
    add(" View capsule", "stub: capsule evidence viewer", theme.vti_fire);

    f.render_widget(
        Paragraph::new(lines).block(empty_block(theme, " Actions ")),
        area,
    );
}

fn draw_placeholder(f: &mut Frame, area: Rect, msg: &str, theme: &Theme) {
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(format!("  {}", msg), theme.muted())),
        ])
        .block(empty_block(theme, "")),
        area,
    );
}

fn row<'a>(label: &str, value: &str, value_style: Style, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {:<10}", label), theme.muted()),
        Span::styled(value.to_string(), value_style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_cycles_next_and_prev() {
        let mut t = InspectorTab::Overview;
        for _ in 0..InspectorTab::ALL.len() {
            t = t.next();
        }
        assert_eq!(t, InspectorTab::Overview);

        let mut t = InspectorTab::Logs;
        t = t.prev();
        assert_eq!(t, InspectorTab::Overview);
    }
}
