//! Owner: Interactive TUI subsystem — mouse handler for the Delivery view.
//! Proof: `cargo nextest run -p jeryu -- tui::runtime::input::mouse`
//! Invariants: Reads delivery_hit_map populated by the renderer; no direct DAG math.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::tui::app::App;
use crate::tui::workflow::hit_map::DeliveryHitMap;
use crate::tui::workflow::minimap::locate_minimap_click;
use crate::tui::workflow::pr_rail::pr_at_column;

/// Pan step in cells per wheel tick.
const WHEEL_STEP: i32 = 3;

pub(crate) fn handle_delivery_mouse(app: &mut App, m: MouseEvent) {
    let x = m.column;
    let y = m.row;

    match m.kind {
        // ─── Wheel: pan viewport ────────────────────────────────────
        MouseEventKind::ScrollUp => {
            if DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y) {
                app.workflow_nav.viewport_y = (app.workflow_nav.viewport_y - WHEEL_STEP).max(0);
            }
        }
        MouseEventKind::ScrollDown => {
            if DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y) {
                let max = (app.workflow_nav.canvas_height
                    - app
                        .delivery_hit_map
                        .canvas
                        .map(|r| r.height as i32)
                        .unwrap_or(0))
                .max(0);
                app.workflow_nav.viewport_y = (app.workflow_nav.viewport_y + WHEEL_STEP).min(max);
            }
        }
        MouseEventKind::ScrollLeft => {
            if DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y) {
                app.workflow_nav.viewport_x = (app.workflow_nav.viewport_x - WHEEL_STEP).max(0);
            }
        }
        MouseEventKind::ScrollRight => {
            if DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y) {
                let max = (app.workflow_nav.canvas_width
                    - app
                        .delivery_hit_map
                        .canvas
                        .map(|r| r.width as i32)
                        .unwrap_or(0))
                .max(0);
                app.workflow_nav.viewport_x = (app.workflow_nav.viewport_x + WHEEL_STEP).min(max);
            }
        }

        // ─── Drag: pan viewport ─────────────────────────────────────
        MouseEventKind::Down(MouseButton::Left) => {
            handle_click(app, x, y);
            // Arm drag origin only when clicking inside the canvas.
            if DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y) {
                app.drag_origin = Some((x, y));
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((ox, oy)) = app.drag_origin {
                let dx = ox as i32 - x as i32;
                let dy = oy as i32 - y as i32;
                let canvas_w = app
                    .delivery_hit_map
                    .canvas
                    .map(|r| r.width as i32)
                    .unwrap_or(0);
                let canvas_h = app
                    .delivery_hit_map
                    .canvas
                    .map(|r| r.height as i32)
                    .unwrap_or(0);
                let max_x = (app.workflow_nav.canvas_width - canvas_w).max(0);
                let max_y = (app.workflow_nav.canvas_height - canvas_h).max(0);
                app.workflow_nav.viewport_x = (app.workflow_nav.viewport_x + dx).clamp(0, max_x);
                app.workflow_nav.viewport_y = (app.workflow_nav.viewport_y + dy).clamp(0, max_y);
                app.drag_origin = Some((x, y));
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.drag_origin = None;
        }
        _ => {}
    }
}

fn handle_click(app: &mut App, x: u16, y: u16) {
    // PR rail click → switch PR.
    if let Some(rail) = app.delivery_hit_map.pr_rail
        && DeliveryHitMap::contains(Some(rail), x, y)
    {
        // Translate to the rail's inner column (skip the border).
        let inner_x = x.saturating_sub(rail.x + 1);
        if let Some(idx) = pr_at_column(&app.delivery_snapshot, inner_x) {
            app.delivery_snapshot.selected_pr_idx = idx;
            // Refresh the mirrored workflow snapshot via the existing path.
            app.refresh_delivery_snapshot();
        }
        return;
    }

    // Minimap click → jump phase/node.
    if let Some(mini) = app.delivery_hit_map.minimap
        && DeliveryHitMap::contains(Some(mini), x, y)
    {
        let inner_x = x.saturating_sub(mini.x + 1);
        let inner_y = y.saturating_sub(mini.y + 1);
        let inner_w = mini.width.saturating_sub(2).max(1);
        let inner_h = mini.height.saturating_sub(2).max(1);
        if let Some((pi, ni)) =
            locate_minimap_click(&app.delivery_snapshot, inner_x, inner_y, inner_w, inner_h)
        {
            app.workflow_nav.phase_idx = pi;
            app.workflow_nav.node_idx = ni;
            let dag_h = app.delivery_hit_map.canvas.map(|r| r.height).unwrap_or(30);
            let dag_w = app.delivery_hit_map.canvas.map(|r| r.width).unwrap_or(120);
            app.workflow_nav.ensure_selected_visible(dag_h, dag_w);
        }
        return;
    }

    // Node card click promotes that node (second click on the promoted
    // node toggles the inspector).
    if let Some((pi, ni)) = app.delivery_hit_map.card_at(x, y) {
        let already = pi == app.workflow_nav.phase_idx && ni == app.workflow_nav.node_idx;
        app.workflow_nav.phase_idx = pi;
        app.workflow_nav.node_idx = ni;
        if already {
            app.workflow_inspect_open = !app.workflow_inspect_open;
        }
    }
}
