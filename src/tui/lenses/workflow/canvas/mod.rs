//! Workflow DAG canvas renderer for the Flight Deck lens.
//!
//! Pure rendering only: this module consumes immutable workflow snapshots,
//! navigation state, and theme data.

mod edges;
mod node;
mod panels;

use ratatui::{Frame, layout::Rect};

pub(crate) use node::node_color;

use crate::tui::lenses::workflow::model::*;
use crate::tui::theme::Theme;
use crate::tui::workflow::{
    hit_map::DeliveryHitMap,
    nav::{EDGE_GUTTER_H, NODE_CARD_H, PHASE_HEADER_H, WorkflowNav},
};

/// Hit-map-aware DAG canvas. Mirrors `draw_dag_canvas` but records each
/// rendered card rect for mouse hit-testing.
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
    draw_canvas_rows(f, dag_area, snapshot, nav, theme, tick, Some(hit_map));
}

/// Render the scrollable DAG canvas inside `dag_area`.
pub fn draw_dag_canvas(
    f: &mut Frame,
    dag_area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
) {
    draw_canvas_rows(f, dag_area, snapshot, nav, theme, tick, None);
}

pub fn draw_workflow_empty_state(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    status: &crate::tui::app::DeliverySourceStatus,
) {
    panels::draw_workflow_empty_state(f, area, theme, status);
}

pub(crate) fn draw_no_pr_state(f: &mut Frame, area: Rect, theme: &Theme) {
    panels::draw_no_pr_state(f, area, theme);
}

pub(crate) fn draw_delivery_footer(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    theme: &Theme,
) {
    panels::draw_delivery_footer(f, area, delivery, theme);
}

pub(crate) fn draw_empty_state(
    f: &mut Frame,
    area: Rect,
    snapshot: &WorkflowSnapshot,
    theme: &Theme,
) {
    panels::draw_empty_state(f, area, snapshot, theme);
}

pub(crate) fn draw_summary_banner(
    f: &mut Frame,
    area: Rect,
    snap: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
) {
    panels::draw_summary_banner(f, area, snap, nav, theme);
}

#[allow(clippy::too_many_arguments)]
fn draw_canvas_rows(
    f: &mut Frame,
    dag_area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    mut hit_map: Option<&mut DeliveryHitMap>,
) {
    if dag_area.height == 0 {
        return;
    }

    for (phase_idx, phase) in snapshot.phases.iter().enumerate() {
        let virtual_y = nav.phase_virtual_y(phase_idx);
        let phase_h = PHASE_HEADER_H as i32 + NODE_CARD_H as i32;
        let screen_y = virtual_y - nav.viewport_y;
        if screen_y + phase_h + EDGE_GUTTER_H as i32 <= 0 || screen_y >= dag_area.height as i32 {
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
                match hit_map.as_deref_mut() {
                    Some(map) => node::draw_phase_row_with_hits(
                        f, phase_rect, phase_idx, phase, snapshot, nav, theme, tick, map,
                    ),
                    None => node::draw_phase_row(
                        f, phase_rect, phase_idx, phase, snapshot, nav, theme, tick,
                    ),
                }
            }
        }

        if phase_idx + 1 < snapshot.phases.len() {
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
                edges::draw_edge_gutter(f, gutter_rect, phase_idx, snapshot, nav, theme);
            }
        }
    }

    panels::draw_viewport_indicator(f, dag_area, nav, theme);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::workflow::builder::build_demo_snapshot;

    #[test]
    fn node_color_maps_all_statuses() {
        let theme = Theme::dark();
        for status in [
            WorkflowStatus::Waiting,
            WorkflowStatus::Running,
            WorkflowStatus::Ran,
            WorkflowStatus::Error,
            WorkflowStatus::Skipped,
            WorkflowStatus::Cached,
            WorkflowStatus::Blocked,
            WorkflowStatus::Unknown,
        ] {
            let _ = node_color(status, &theme);
        }
    }

    #[test]
    fn demo_snapshot_has_phases() {
        let snap = build_demo_snapshot();
        assert!(!snap.phases.is_empty());
        assert!(!snap.nodes.is_empty());
    }
}
