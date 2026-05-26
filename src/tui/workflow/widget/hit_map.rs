//! Owner: Interactive TUI subsystem — workflow widget hit-map registration.
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::`
//! Invariants: Mirrors `render` variants but registers card rects into the
//! `DeliveryHitMap` for mouse hit-testing. Still pure rendering — only the
//! hit-map struct is mutated.
//! Layout: split from monolithic `widget.rs` (1063 LOC) per TUI_RESET_PLAN_FINAL.md §3.2.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::super::hit_map::DeliveryHitMap;
use super::super::model::*;
use super::super::nav::{EDGE_GUTTER_H, NODE_CARD_H, NODE_CARD_W, PHASE_HEADER_H, WorkflowNav};
use super::layout::{draw_edge_gutter, draw_viewport_indicator};
use super::render::draw_node_card;
use crate::tui::theme::Theme;

/// Hit-map-aware DAG canvas. Mirrors `draw_dag_canvas` but pushes each
/// rendered card's rect into `hit_map` for mouse hit-testing.
#[allow(clippy::too_many_arguments)]
pub fn draw_dag_canvas_with_hits(
    f: &mut Frame,
    dag_area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
) {
    let dag_h = dag_area.height;
    if dag_h == 0 {
        return;
    }

    for (pi, phase) in snapshot.phases.iter().enumerate() {
        let virtual_y = nav.phase_virtual_y(pi);
        let phase_h = PHASE_HEADER_H as i32 + NODE_CARD_H as i32;
        let screen_y = virtual_y - nav.viewport_y;
        if screen_y + phase_h + EDGE_GUTTER_H as i32 <= 0 || screen_y >= dag_h as i32 {
            continue;
        }
        let render_y = dag_area.y as i32 + screen_y;
        if render_y >= 0 && (render_y as u16) < dag_area.y + dag_area.height {
            let clipped_y = render_y.max(dag_area.y as i32) as u16;
            let max_bottom = dag_area.y + dag_area.height;
            let clipped_h =
                ((render_y + phase_h).min(max_bottom as i32) - clipped_y as i32).max(0) as u16;
            if clipped_h > 0 {
                let phase_rect = Rect::new(dag_area.x, clipped_y, dag_area.width, clipped_h);
                draw_phase_row_with_hits(
                    f, phase_rect, pi, phase, snapshot, nav, theme, tick, hit_map,
                );
            }
        }
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
    draw_viewport_indicator(f, dag_area, nav, theme);
}

#[allow(clippy::too_many_arguments)]
fn draw_phase_row_with_hits(
    f: &mut Frame,
    area: Rect,
    phase_idx: usize,
    phase: &WorkflowPhase,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
) {
    let card_w = NODE_CARD_W;
    let spacing = 2u16;

    // Phase header line.
    let phase_title = format!(" {} ", phase.title);
    let header_style = Style::default().fg(theme.border_subtle);
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
    let cards_y = area.y + 1;
    let cards_h = area.height.saturating_sub(1);
    if cards_h == 0 {
        return;
    }
    for (ni, node_id) in phase.node_ids.iter().enumerate() {
        if let Some(node) = snap.node(node_id) {
            let vx = (ni as i32) * (card_w as i32 + spacing as i32);
            let screen_x = vx - nav.viewport_x;
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
            hit_map.cards.push((card_rect, phase_idx, ni));
        }
    }
}
