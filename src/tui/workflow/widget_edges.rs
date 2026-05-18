//! Owner: Interactive TUI subsystem — workflow edge gutter renderer
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::widget`
//! Invariants: Pure rendering for inter-phase dependency edges (ASCII art connectors).

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
};

use super::model::*;
use super::nav::{NODE_CARD_W, WorkflowNav};
use crate::tui::theme::Theme;

/// Draw ASCII dependency edges in the gutter between two adjacent phases.
pub(crate) fn draw_edge_gutter(
    f: &mut Frame,
    area: Rect,
    phase_idx: usize,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let card_w = NODE_CARD_W;
    let spacing = 2u16;

    let current_phase = match snap.phases.get(phase_idx) {
        Some(p) => p,
        None => return,
    };
    let next_phase = match snap.phases.get(phase_idx + 1) {
        Some(p) => p,
        None => return,
    };

    let buf = f.buffer_mut();

    for (ni, next_nid) in next_phase.node_ids.iter().enumerate() {
        let next_node = match snap.node(next_nid) {
            Some(n) => n,
            None => continue,
        };

        // Target X center for this next-phase node.
        let next_vx = (ni as i32) * (card_w as i32 + spacing as i32) + card_w as i32 / 2;
        let next_sx = next_vx - nav.viewport_x;
        if next_sx < 0 || next_sx >= area.width as i32 {
            continue;
        }
        let target_x = area.x + next_sx as u16;

        // Find parent nodes in the current phase.
        for (pi, parent_nid) in current_phase.node_ids.iter().enumerate() {
            if !next_node.deps.contains(parent_nid) {
                let has_edge = snap
                    .edges
                    .iter()
                    .any(|e| e.from == *parent_nid && e.to == *next_nid);
                if !has_edge {
                    continue;
                }
            }

            let parent_vx = (pi as i32) * (card_w as i32 + spacing as i32) + card_w as i32 / 2;
            let parent_sx = parent_vx - nav.viewport_x;
            if parent_sx < 0 || parent_sx >= area.width as i32 {
                continue;
            }
            let source_x = area.x + parent_sx as u16;

            let edge_color = edge_color_for(next_node, theme);
            let style = Style::default().fg(edge_color);

            let y0 = area.y;
            let y_mid = area.y + area.height / 2;
            let y_end = area.y + area.height.saturating_sub(1);

            if source_x < area.x + area.width && y0 < area.y + area.height {
                set_cell(buf, source_x, y0, "│", style, area);
            }

            if source_x != target_x && y_mid < area.y + area.height {
                let (left, right) = if source_x < target_x {
                    (source_x, target_x)
                } else {
                    (target_x, source_x)
                };

                if source_x < target_x {
                    set_cell(buf, source_x, y_mid, "└", style, area);
                } else {
                    set_cell(buf, source_x, y_mid, "┘", style, area);
                }

                for x in (left + 1)..right {
                    set_cell(buf, x, y_mid, "─", style, area);
                }

                if source_x < target_x {
                    set_cell(buf, target_x, y_mid, "┐", style, area);
                } else {
                    set_cell(buf, target_x, y_mid, "┌", style, area);
                }

                for y in (y_mid + 1)..=y_end {
                    set_cell(buf, target_x, y, "│", style, area);
                }
            } else {
                for y in (y0 + 1)..=y_end {
                    set_cell(buf, source_x, y, "│", style, area);
                }
            }

            set_cell(buf, target_x, y_end, "▼", style, area);
        }
    }
}

/// Safely set a cell in the buffer, clipped to the given area.
fn set_cell(
    buf: &mut ratatui::buffer::Buffer,
    x: u16,
    y: u16,
    symbol: &str,
    style: Style,
    clip: Rect,
) {
    if x >= clip.x && x < clip.x + clip.width && y >= clip.y && y < clip.y + clip.height {
        let cell = &mut buf[(x, y)];
        cell.set_symbol(symbol);
        cell.set_style(style);
    }
}

/// Choose edge color based on the target node's status.
fn edge_color_for(node: &WorkflowNode, theme: &Theme) -> Color {
    match node.status {
        WorkflowStatus::Running => theme.running,
        WorkflowStatus::Error => theme.fail,
        WorkflowStatus::Blocked => theme.blocked,
        WorkflowStatus::Ran => theme.ok,
        WorkflowStatus::Waiting => theme.border_subtle,
        WorkflowStatus::Skipped => theme.skipped,
        WorkflowStatus::Cached => theme.vti_fire,
        WorkflowStatus::Unknown => theme.text_muted,
    }
}
