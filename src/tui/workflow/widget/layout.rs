//! Owner: Interactive TUI subsystem — workflow widget layout & geometry.
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::`
//! Invariants: Region/gutter geometry, edge connectors, viewport indicator.
//! All pure rendering — no mutation of workflow state, no I/O.
//! Layout: split from monolithic `widget.rs` (1063 LOC) per TUI_RESET_PLAN_FINAL.md §3.2.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::model::*;
use super::super::nav::{EDGE_GUTTER_H, NODE_CARD_H, NODE_CARD_W, PHASE_HEADER_H, WorkflowNav};
use crate::tui::{focus::PaneChrome, theme::Theme};

/// Height of one full phase row on the virtual canvas
/// (header + card body + edge gutter below).
/// Height of one full phase row on the virtual canvas.
#[allow(dead_code)]
pub(super) const _PHASE_ROW_H: i32 =
    PHASE_HEADER_H as i32 + NODE_CARD_H as i32 + EDGE_GUTTER_H as i32;

pub(super) fn draw_canvas_frame(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    chrome: Option<PaneChrome>,
) -> Rect {
    let title = match chrome {
        Some(chrome) => chrome.title("Canvas"),
        None => crate::tui::focus::title_with_esc("Canvas", false),
    };
    let border_style = match chrome {
        Some(chrome) => chrome.border_style,
        None => Style::default().fg(theme.border_subtle),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

pub(super) fn visible(r: Rect) -> Option<Rect> {
    if r.width == 0 || r.height == 0 {
        None
    } else {
        Some(r)
    }
}

/// Draw ASCII dependency edges in the gutter between two adjacent phases.
pub(super) fn draw_edge_gutter(
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

    // For each node in the next phase, find its dependencies in the current phase
    // and draw connectors.
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
                // Also check edges for stage-order dependencies.
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

            // Draw vertical line from source down.
            let y0 = area.y;
            let y_mid = area.y + area.height / 2;
            let y_end = area.y + area.height.saturating_sub(1);

            // Vertical drop from parent.
            if source_x < area.x + area.width && y0 < area.y + area.height {
                set_cell(buf, source_x, y0, "│", style, area);
            }

            // Horizontal connector.
            if source_x != target_x && y_mid < area.y + area.height {
                let (left, right) = if source_x < target_x {
                    (source_x, target_x)
                } else {
                    (target_x, source_x)
                };

                // Corner at source.
                if source_x < target_x {
                    set_cell(buf, source_x, y_mid, "└", style, area);
                } else {
                    set_cell(buf, source_x, y_mid, "┘", style, area);
                }

                // Horizontal line.
                for x in (left + 1)..right {
                    set_cell(buf, x, y_mid, "─", style, area);
                }

                // Corner at target.
                if source_x < target_x {
                    set_cell(buf, target_x, y_mid, "┐", style, area);
                } else {
                    set_cell(buf, target_x, y_mid, "┌", style, area);
                }

                // Vertical drop to target.
                for y in (y_mid + 1)..=y_end {
                    set_cell(buf, target_x, y, "│", style, area);
                }
            } else {
                // Straight vertical.
                for y in (y0 + 1)..=y_end {
                    set_cell(buf, source_x, y, "│", style, area);
                }
            }

            // Arrow head at the bottom.
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

/// Draw a small viewport position indicator in the bottom-right corner.
pub(super) fn draw_viewport_indicator(f: &mut Frame, area: Rect, nav: &WorkflowNav, theme: &Theme) {
    if nav.canvas_height <= 0 || area.height == 0 {
        return;
    }

    let y_pct = if nav.canvas_height > 0 {
        ((nav.viewport_y as f64 + area.height as f64 / 2.0) / nav.canvas_height as f64 * 100.0)
            .clamp(0.0, 100.0) as u16
    } else {
        0
    };

    let x_pct = if nav.canvas_width > 0 {
        ((nav.viewport_x as f64 + area.width as f64 / 2.0) / nav.canvas_width as f64 * 100.0)
            .clamp(0.0, 100.0) as u16
    } else {
        0
    };

    let indicator = format!(" ↕{}% ↔{}% ", y_pct, x_pct);
    let iw = indicator.len() as u16;

    if area.width > iw + 2 && area.height > 1 {
        let ix = area.x + area.width - iw - 1;
        let iy = area.y + area.height - 1;
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                indicator,
                Style::default().fg(theme.text_muted).bg(theme.bg_surface),
            ))),
            Rect::new(ix, iy, iw, 1),
        );
    }
}
