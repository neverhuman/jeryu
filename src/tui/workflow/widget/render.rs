//! Owner: Interactive TUI subsystem — workflow DAG render primitives.
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::`
//! Invariants: Pure rendering — no mutation of workflow state, no I/O.
//! Layout: split from monolithic `widget.rs` (1063 LOC) per TUI_RESET_PLAN_FINAL.md §3.2.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::model::*;
use super::super::nav::{
    EDGE_GUTTER_H, NODE_CARD_H, NODE_CARD_W, PHASE_HEADER_H, WorkflowNav, WorkflowZoom,
};
use super::layout::{draw_edge_gutter, draw_viewport_indicator};
use crate::tui::theme::Theme;

/// Render the scrollable DAG canvas inside `dag_area`. Reused by both the
/// legacy workflow tab and the new Delivery view.
pub fn draw_dag_canvas(
    f: &mut Frame,
    dag_area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
) {
    let dag_h = dag_area.height;
    if dag_h == 0 {
        return;
    }

    // Render phases with viewport clipping.
    for (pi, phase) in snapshot.phases.iter().enumerate() {
        let virtual_y = nav.phase_virtual_y(pi); // DAG-relative
        let phase_h = PHASE_HEADER_H as i32 + NODE_CARD_H as i32;

        // Check if this phase is visible in the viewport.
        let screen_y = virtual_y - nav.viewport_y;
        if screen_y + phase_h + EDGE_GUTTER_H as i32 <= 0 || screen_y >= dag_h as i32 {
            continue; // Entirely off-screen.
        }

        // Compute the visible portion of this phase.
        let render_y = dag_area.y as i32 + screen_y;
        if render_y >= 0 && (render_y as u16) < dag_area.y + dag_area.height {
            let clipped_y = render_y.max(dag_area.y as i32) as u16;
            let max_bottom = dag_area.y + dag_area.height;
            let clipped_h =
                ((render_y + phase_h).min(max_bottom as i32) - clipped_y as i32).max(0) as u16;
            if clipped_h > 0 {
                let phase_rect = Rect::new(dag_area.x, clipped_y, dag_area.width, clipped_h);
                draw_phase_row(f, phase_rect, pi, phase, snapshot, nav, theme, tick);
            }
        }

        // Draw edge gutter below this phase (if there's a next phase).
        if pi + 1 < snapshot.phases.len() {
            let gutter_y = render_y + phase_h;
            if gutter_y >= dag_area.y as i32
                && (gutter_y as u16) + EDGE_GUTTER_H <= dag_area.y + dag_area.height
            {
                let gutter_rect = Rect::new(
                    dag_area.x,
                    gutter_y as u16,
                    dag_area.width,
                    EDGE_GUTTER_H.min(dag_area.y + dag_area.height - gutter_y as u16),
                );
                draw_edge_gutter(f, gutter_rect, pi, snapshot, nav, theme);
            }
        }
    }

    // --- Viewport position indicator ---
    draw_viewport_indicator(f, dag_area, nav, theme);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_phase_row(
    f: &mut Frame,
    area: Rect,
    phase_idx: usize,
    phase: &WorkflowPhase,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
) {
    let _node_count = phase.node_ids.len().max(1);

    // Compute card width: fixed NODE_CARD_W, laid out with spacing.
    let card_w = NODE_CARD_W;
    let spacing = 2u16;

    // Phase header line.
    let phase_title = format!(" {} ", phase.title);
    let header_style = Style::default().fg(theme.border_subtle);
    // Build a dashed line header.
    let dashes: String =
        "─".repeat(area.width.saturating_sub(phase_title.len() as u16 + 2) as usize);
    let header_line = Line::from(vec![
        Span::styled("─", header_style),
        Span::styled(
            phase_title.clone(),
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::DIM),
        ),
        Span::styled(dashes, header_style),
    ]);
    if area.height > 0 {
        f.render_widget(
            Paragraph::new(header_line),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    // Render node cards below the header.
    let cards_y = area.y + 1;
    let cards_h = area.height.saturating_sub(1);
    if cards_h == 0 {
        return;
    }

    for (ni, node_id) in phase.node_ids.iter().enumerate() {
        if let Some(node) = snap.node(node_id) {
            // Virtual X position for this card.
            let vx = (ni as i32) * (card_w as i32 + spacing as i32);
            let screen_x = vx - nav.viewport_x;

            // Skip if entirely off-screen horizontally.
            if screen_x + card_w as i32 <= 0 || screen_x >= area.width as i32 {
                continue;
            }

            let render_x = (area.x as i32 + screen_x).max(area.x as i32) as u16;
            let available_w = (area.x + area.width).saturating_sub(render_x);
            let cw = card_w.min(available_w);
            if cw == 0 {
                continue;
            }

            let card_rect = Rect::new(render_x, cards_y, cw, cards_h.min(NODE_CARD_H - 1));
            let is_selected = phase_idx == nav.phase_idx && ni == nav.node_idx;
            draw_node_card(f, card_rect, node, is_selected, snap, theme, tick, nav.zoom);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_node_card(
    f: &mut Frame,
    area: Rect,
    node: &WorkflowNode,
    is_selected: bool,
    snap: &WorkflowSnapshot,
    theme: &Theme,
    tick: u64,
    zoom: WorkflowZoom,
) {
    let status_color = node_color(node.status, theme);
    let stalled = is_stalled(node, chrono::Utc::now());

    // Border style: selected → bright accent; stalled running → amber pulse;
    // running → green pulse; otherwise → status color.
    let border_style = if is_selected {
        Style::default()
            .fg(theme.border_accent)
            .add_modifier(Modifier::BOLD)
    } else if stalled {
        let pulse = if (tick / 2).is_multiple_of(2) {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        Style::default().fg(theme.warning).add_modifier(pulse)
    } else if node.status == WorkflowStatus::Running {
        let pulse = if (tick / 4).is_multiple_of(2) {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        Style::default().fg(status_color).add_modifier(pulse)
    } else {
        Style::default().fg(status_color)
    };

    let vti_badge = match node.vti_status.as_ref() {
        Some(v) => v.badge(),
        None => "",
    };
    let cache_badge = match node.cache_verdict.as_ref() {
        Some(c) => c.badge(),
        None => "",
    };

    // Accent prefix from the node kind (agent, auto-merge, promote, etc.).
    let accent = node.kind.glyph();
    let title_max = area.width.saturating_sub(22) as usize;
    let title = format!(
        " {} {} {} {}{}{}{}",
        node.status.glyph(),
        if accent.is_empty() { " " } else { accent },
        node.status.label(),
        crate::tui::widgets::truncate_label(&node.label, title_max),
        if node.critical_path { " [CRIT]" } else { "" },
        if is_selected { " [SEL]" } else { "" },
        if stalled { " [STALL]" } else { "" },
    );

    let mut lines = Vec::new();

    // Command line — hidden in Overview/Dense zoom levels.
    if zoom == WorkflowZoom::Cards
        && let Some(cmd) = &node.command
    {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                crate::tui::widgets::truncate_label(cmd, area.width.saturating_sub(4) as usize)
            ),
            theme.muted(),
        )));
    }

    // Badges + progress.
    let mut badge_spans = vec![Span::styled("  ", Style::default())];
    if !vti_badge.is_empty() {
        badge_spans.push(Span::styled(
            format!("{} ", vti_badge),
            theme.bold(theme.vti_fire),
        ));
    }
    if !cache_badge.is_empty() {
        badge_spans.push(Span::styled(
            format!("{} ", cache_badge),
            theme.bold(theme.ok),
        ));
    }
    if let Some(pct) = node.progress_pct {
        badge_spans.push(Span::styled(
            format!("{} {}%", progress_bar(pct, 10), pct),
            theme.bold(status_color),
        ));
    }
    if let Some(eta) = node.eta_secs {
        badge_spans.push(Span::styled(format!(" eta:{}s", eta), theme.muted()));
    }
    if node.kind.is_rollback_eligible()
        && matches!(node.status, WorkflowStatus::Ran | WorkflowStatus::Running)
    {
        badge_spans.push(Span::styled(" [ROLLBACK]", theme.bold(theme.warning)));
    }
    if matches!(node.kind, WorkflowNodeKind::AgentReview { .. }) {
        badge_spans.push(Span::styled(" agent", theme.bold(theme.agent)));
    }
    // Intelligence chip: failed/blocked nodes show "blocks N · reason".
    if matches!(node.status, WorkflowStatus::Error | WorkflowStatus::Blocked) {
        let downstream =
            crate::tui::workflow::intelligence::compute_downstream_impact(snap, &node.id);
        let reason_excerpt = match node.reason.as_deref() {
            Some(reason) => {
                let max = area.width.saturating_sub(20) as usize;
                reason.chars().take(max.max(6)).collect::<String>()
            }
            None => String::new(),
        };
        let chip = if downstream > 0 {
            format!(" ⚠ blocks {}", downstream)
        } else {
            " ⚠".into()
        };
        badge_spans.push(Span::styled(chip, theme.bold(theme.fail)));
        if !reason_excerpt.is_empty() {
            badge_spans.push(Span::raw(" "));
            badge_spans.push(Span::styled(reason_excerpt, theme.muted()));
        }
    }
    // Overview zoom hides badges entirely; Cards/Dense always show them.
    if zoom != WorkflowZoom::Overview && badge_spans.len() > 1 {
        lines.push(Line::from(badge_spans));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        ),
        area,
    );
}

/// Render a block-character progress bar (e.g. `███░░░░ 41%` style).
fn progress_bar(pct: u16, width: usize) -> String {
    let pct = pct.min(100) as usize;
    let filled = pct * width / 100;
    let empty = width - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Determine whether a running node has overrun its ETA budget (eta*1.5,
/// 90s floor) using the same logic as `intelligence::detect_stalls`.
fn is_stalled(node: &WorkflowNode, now: chrono::DateTime<chrono::Utc>) -> bool {
    if !matches!(node.status, WorkflowStatus::Running) {
        return false;
    }
    let Some(started) = node.started_at else {
        return false;
    };
    let elapsed = (now - started).num_seconds().max(0) as u64;
    let budget = node
        .eta_secs
        .map(|e| ((e as f64) * 1.5) as u64)
        .unwrap_or(90)
        .max(90);
    elapsed > budget
}

pub(super) fn node_color(status: WorkflowStatus, theme: &Theme) -> Color {
    match status {
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Waiting => theme.waiting,
        WorkflowStatus::Skipped => theme.skipped,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Unknown => theme.text_muted,
    }
}
