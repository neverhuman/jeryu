//! Owner: Interactive TUI subsystem — mouse handler
//! Proof: `cargo nextest run -p jeryu -- tui::runtime::input::mouse`
//! Invariants: Reads hit maps populated by the renderer; no direct DAG math.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::tui::{
    app::App,
    focus::{self, PaneId},
    workflow::hit_map::DeliveryHitMap,
    workflow::minimap::locate_minimap_click,
    workflow::pr_rail::pr_at_column,
};

/// Pan step in cells per wheel tick.
const WHEEL_STEP: i32 = 3;

pub(crate) fn handle(app: &mut App, m: MouseEvent) {
    let x = m.column;
    let y = m.row;

    match m.kind {
        MouseEventKind::ScrollUp => {
            if app.focus.fullscreen == Some(PaneId::ActivityLog(app.active_tab)) {
                app.scroll_logs_up(3);
            } else if app.active_tab == crate::tui::app::ActiveTab::Workflow
                && DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y)
            {
                app.workflow_nav.viewport_y = (app.workflow_nav.viewport_y - WHEEL_STEP).max(0);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.focus.fullscreen == Some(PaneId::ActivityLog(app.active_tab)) {
                app.scroll_logs_down(3);
            } else if app.active_tab == crate::tui::app::ActiveTab::Workflow
                && DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y)
            {
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
            if app.active_tab == crate::tui::app::ActiveTab::Workflow
                && DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y)
            {
                app.workflow_nav.viewport_x = (app.workflow_nav.viewport_x - WHEEL_STEP).max(0);
            }
        }
        MouseEventKind::ScrollRight => {
            if app.active_tab == crate::tui::app::ActiveTab::Workflow
                && DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y)
            {
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
        MouseEventKind::Down(MouseButton::Left) => {
            handle_click(app, x, y);

            if app.active_tab == crate::tui::app::ActiveTab::Workflow
                && DeliveryHitMap::contains(app.delivery_hit_map.canvas, x, y)
            {
                app.drag_origin = Some((x, y));
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.active_tab == crate::tui::app::ActiveTab::Workflow
                && let Some((ox, oy)) = app.drag_origin
            {
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
    if let Some(pane) = app.focus_map.esc_at(x, y)
        && focus::should_show_esc(app, pane)
    {
        let _ = app.close_focus_overlay();
        return;
    }

    if let Some(pane) = app.focus_map.pane_at(x, y) {
        if app.focus.active != pane {
            app.focus.stack.clear();
            app.focus.fullscreen = None;
            app.maximize_logs = false;
            app.focus.active = pane;
        } else if !app.focus.is_drilled() {
            app.enter_focused_pane();
        }
    }

    if app.active_tab == crate::tui::app::ActiveTab::Workflow {
        // PR rail click → switch PR.
        if let Some(rail) = app.delivery_hit_map.pr_rail
            && DeliveryHitMap::contains(Some(rail), x, y)
        {
            let inner_x = x.saturating_sub(rail.x + 1);
            if let Some(idx) = pr_at_column(&app.delivery_snapshot, inner_x) {
                app.delivery_snapshot.selected_pr_idx = idx;
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
}
