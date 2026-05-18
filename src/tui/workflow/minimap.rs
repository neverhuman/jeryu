//! Owner: Interactive TUI subsystem — Delivery minimap (bird's-eye DAG)
//! Proof: rendered indirectly via integration smoke
//! Invariants: Render-only; one block-character per node in the selected PR.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::model::*;
use super::nav::WorkflowNav;
use crate::tui::theme::Theme;

pub fn draw_minimap(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = Block::default()
        .title(" Map ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(pr) = delivery.selected() else {
        return;
    };

    let inner_w = inner.width as usize;
    let inner_h = inner.height as usize;
    if inner_w == 0 || inner_h == 0 {
        return;
    }

    // Map each phase to a row, each node within the phase to a column.
    let phase_count = pr.snapshot.phases.len();
    if phase_count == 0 {
        return;
    }

    let max_nodes = pr
        .snapshot
        .phases
        .iter()
        .map(|p| p.node_ids.len())
        .max()
        .unwrap_or(1);

    let mut lines: Vec<Line> = Vec::with_capacity(inner_h);
    for row in 0..inner_h {
        // Map terminal row → phase index. Linearly compress phases into rows.
        let phase_idx = if phase_count >= inner_h {
            row * phase_count / inner_h
        } else {
            row * phase_count / inner_h.max(1)
        };
        if phase_idx >= phase_count {
            break;
        }
        let phase = &pr.snapshot.phases[phase_idx];
        let is_current_phase = phase_idx == nav.phase_idx;

        let mut spans: Vec<Span> = Vec::with_capacity(inner_w);
        for col in 0..inner_w {
            let node_idx = col * max_nodes / inner_w.max(1);
            if node_idx < phase.node_ids.len() {
                let nid = &phase.node_ids[node_idx];
                let node = pr.snapshot.node(nid);
                let color = node
                    .map(|n| status_color(n.status, theme))
                    .unwrap_or(theme.text_muted);
                let glyph = if is_current_phase && node_idx == nav.node_idx {
                    "▣"
                } else {
                    "█"
                };
                let mut style = Style::default().fg(color);
                if is_current_phase && node_idx == nav.node_idx {
                    style = style.add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(glyph, style));
            } else {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Translate a click on the minimap into a (phase_idx, node_idx) selection.
/// `relative` is the click position inside the rail's inner rect.
pub fn locate_minimap_click(
    delivery: &DeliverySnapshot,
    relative_x: u16,
    relative_y: u16,
    inner_w: u16,
    inner_h: u16,
) -> Option<(usize, usize)> {
    let pr = delivery.selected()?;
    let phase_count = pr.snapshot.phases.len();
    if phase_count == 0 {
        return None;
    }
    let max_nodes = pr
        .snapshot
        .phases
        .iter()
        .map(|p| p.node_ids.len())
        .max()
        .unwrap_or(1);
    let phase_idx = (relative_y as usize) * phase_count / inner_h.max(1) as usize;
    if phase_idx >= phase_count {
        return None;
    }
    let node_idx = (relative_x as usize) * max_nodes / inner_w.max(1) as usize;
    let phase = &pr.snapshot.phases[phase_idx];
    if node_idx >= phase.node_ids.len() {
        return None;
    }
    Some((phase_idx, node_idx))
}

fn status_color(status: WorkflowStatus, theme: &Theme) -> Color {
    match status {
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Skipped => theme.skipped,
        WorkflowStatus::Waiting => theme.waiting,
        WorkflowStatus::Unknown => theme.text_muted,
    }
}
