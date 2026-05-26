//! Owner: Interactive TUI subsystem — workflow inspect overlay
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Overlay rendering is read-only; no control-plane mutations.
//! U14 (first-cut): extracted from src/tui/ui.rs (no behaviour changes).
//!
//! Centered modal overlay used as the narrow-terminal fallback for the
//! workflow inspect side-pane. When the terminal width is below
//! `INSPECTOR_MIN_TERM_W` (or the side-pane is otherwise unavailable), the
//! body dispatcher renders this overlay over the canvas instead.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::tui::app::App;

/// Draw a centered overlay with full detail for the selected workflow node.
pub(super) fn draw_workflow_inspect_overlay(f: &mut Frame, app: &App) {
    let theme = crate::tui::theme::Theme::dark();
    let area = f.area();

    // Center a box covering ~60% of the screen.
    let overlay_w = (area.width * 3 / 5)
        .max(50)
        .min(area.width.saturating_sub(4));
    let overlay_h = (area.height * 3 / 5)
        .max(16)
        .min(area.height.saturating_sub(4));
    let ox = area.x + (area.width.saturating_sub(overlay_w)) / 2;
    let oy = area.y + (area.height.saturating_sub(overlay_h)) / 2;
    let overlay_area = Rect::new(ox, oy, overlay_w, overlay_h);

    f.render_widget(Clear, overlay_area);

    let selected_id = app.workflow_nav.selected_node_id(&app.workflow_snapshot);
    let node = selected_id.and_then(|id| app.workflow_snapshot.node(id));

    let mut lines = Vec::new();

    if let Some(node) = node {
        let status_color = match node.status {
            crate::tui::workflow::model::WorkflowStatus::Ran => theme.ok,
            crate::tui::workflow::model::WorkflowStatus::Running => theme.running,
            crate::tui::workflow::model::WorkflowStatus::Error => theme.fail,
            crate::tui::workflow::model::WorkflowStatus::Waiting => theme.waiting,
            crate::tui::workflow::model::WorkflowStatus::Blocked => theme.blocked,
            _ => theme.text_secondary,
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", node.status.glyph()),
                theme.bold(status_color),
            ),
            Span::styled(
                node.label.clone(),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("  Status:   ", theme.muted()),
            Span::styled(node.status.label(), theme.bold(status_color)),
            if node.critical_path {
                Span::styled("  [CRITICAL PATH]", theme.bold(theme.fail))
            } else {
                Span::raw("")
            },
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Kind:     ", theme.muted()),
            Span::styled(node.kind.label(), theme.secondary()),
        ]));

        if let Some(cmd) = &node.command {
            lines.push(Line::from(vec![
                Span::styled("  Command:  ", theme.muted()),
                Span::styled(cmd.clone(), theme.primary()),
            ]));
        }

        if !node.deps.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Deps:     ", theme.muted()),
                Span::styled(node.deps.join(", "), theme.secondary()),
            ]));
        }

        if let Some(pct) = node.progress_pct {
            lines.push(Line::from(vec![
                Span::styled("  Progress: ", theme.muted()),
                Span::styled(format!("{}%", pct), theme.bold(status_color)),
            ]));
        }

        if let Some(eta) = node.eta_secs {
            lines.push(Line::from(vec![
                Span::styled("  ETA:      ", theme.muted()),
                Span::styled(format!("{}s", eta), theme.secondary()),
            ]));
        }

        if let Some(dur) = node.duration_secs {
            lines.push(Line::from(vec![
                Span::styled("  Duration: ", theme.muted()),
                Span::styled(format!("{:.1}s", dur), theme.secondary()),
            ]));
        }

        if let Some(ref vti) = node.vti_status {
            lines.push(Line::from(vec![
                Span::styled("  VTI:      ", theme.muted()),
                Span::styled(vti.badge().to_string(), theme.bold(theme.vti_fire)),
            ]));
        }

        if let Some(ref cache) = node.cache_verdict {
            lines.push(Line::from(vec![
                Span::styled("  Cache:    ", theme.muted()),
                Span::styled(cache.badge().to_string(), theme.bold(theme.ok)),
            ]));
        }

        if let Some(ref reason) = node.reason {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Reason:   ", theme.muted()),
                Span::styled(reason.clone(), theme.secondary()),
            ]));
        }

        if !node.tags.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Tags:     ", theme.muted()),
                Span::styled(node.tags.join(", "), theme.secondary()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press Enter or Esc to close",
            theme.muted(),
        )));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No node selected",
            theme.muted(),
        )));
    }

    let title = match node {
        Some(n) => format!(" [ Inspect: {} ] ", n.id),
        None => " [ Inspect ] ".to_string(),
    };

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_accent)),
            )
            .wrap(Wrap { trim: false }),
        overlay_area,
    );
}
