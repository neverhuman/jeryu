//! Owner: Interactive TUI subsystem — rendering body dispatcher
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Body dispatch redacts sensitive text and does not perform
//! control-plane mutations directly. Each per-tab arm delegates to the
//! existing `ui_panels_body*` / `workflow::widget` panels — no rendering
//! logic is duplicated here.
//! U14 (first-cut): extracted from src/tui/ui.rs (no behaviour changes).

use ratatui::{Frame, layout::Rect};

use crate::tui::{
    app::{ActiveTab, App},
    focus::{self, PaneId},
};

use super::overlay::draw_workflow_inspect_overlay;
use super::ui_panels::{
    draw_agents_tab, draw_approvals_tab, draw_bugs_tab, draw_cache_dashboard,
    draw_command_palette as panels_draw_command_palette, draw_evidence_tab, draw_git_tab,
    draw_help_overlay as panels_draw_help_overlay, draw_jobs_tab, draw_llms_tab,
    draw_mission_tab, draw_pools_tab, draw_release_tab, draw_secrets_tab, draw_tests_tab,
};

/// Render the body (middle pane) for the currently active tab.
///
/// For all tabs except `Workflow`, the area is forwarded directly to the
/// matching `draw_*_tab` panel. The `Workflow` arm performs additional
/// inline-inspector layout work and may render an Esc-hotspot focus pane
/// or fall back to a centered modal overlay on narrow terminals.
pub(super) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    match app.active_tab {
        ActiveTab::Workflow => draw_workflow_body(f, app, area),
        ActiveTab::Mission => draw_mission_tab(f, app, area),
        ActiveTab::Release => draw_release_tab(f, app, area),
        ActiveTab::Approvals => draw_approvals_tab(f, app, area),
        ActiveTab::Jobs => draw_jobs_tab(f, app, area),
        ActiveTab::Agents => draw_agents_tab(f, app, area),
        ActiveTab::Tests => draw_tests_tab(f, app, area),
        ActiveTab::Pools => draw_pools_tab(f, app, area),
        ActiveTab::Cache => draw_cache_dashboard(f, app, area),
        ActiveTab::Evidence => draw_evidence_tab(f, app, area),
        ActiveTab::Bugs => draw_bugs_tab(f, app, area),
        ActiveTab::LLMs => draw_llms_tab(f, app, area),
        ActiveTab::Git => draw_git_tab(f, app, area),
        ActiveTab::Secrets => draw_secrets_tab(f, app, area),
    }
}

/// Pass-through for the command palette overlay (defined in ui_panels chain).
pub(super) fn draw_command_palette(f: &mut Frame, app: &App) {
    panels_draw_command_palette(f, app);
}

/// Pass-through for the help overlay (defined in ui_panels chain).
pub(super) fn draw_help_overlay(f: &mut Frame, app: &App) {
    panels_draw_help_overlay(f, app);
}

/// Workflow tab body: refresh snapshot, decide inline-vs-modal inspector,
/// dispatch to the appropriate `workflow::widget::draw_*` painter, register
/// focus panes, and either draw the side-pane inspector or fall back to the
/// centered modal overlay below.
fn draw_workflow_body(f: &mut Frame, app: &mut App, main_area: Rect) {
    // Refresh the Delivery snapshot and the mirrored workflow snapshot
    // for the legacy nav helpers.
    app.refresh_workflow_snapshot();
    let theme = crate::tui::theme::Theme::dark();

    use crate::tui::workflow::inspector::{
        INSPECTOR_MIN_TERM_W, INSPECTOR_W, draw_inspector_pane,
    };
    // Show the side-pane inspector when open AND there's room. Otherwise
    // fall back to the legacy modal overlay (rendered below).
    let inline_pane = app.workflow_inspect_open
        && main_area.width >= INSPECTOR_MIN_TERM_W
        && !app.delivery_snapshot.pull_requests.is_empty();
    let (delivery_area, inspector_area) = if inline_pane {
        let canvas_w = main_area.width.saturating_sub(INSPECTOR_W);
        (
            Rect::new(main_area.x, main_area.y, canvas_w, main_area.height),
            Some(Rect::new(
                main_area.x + canvas_w,
                main_area.y,
                INSPECTOR_W,
                main_area.height,
            )),
        )
    } else {
        (main_area, None)
    };

    if app.delivery_snapshot.pull_requests.is_empty() {
        crate::tui::workflow::widget::draw_workflow_tab(
            f,
            delivery_area,
            &app.workflow_snapshot,
            &app.workflow_nav,
            &theme,
            app.tick_count,
        );
        app.delivery_hit_map = crate::tui::workflow::hit_map::DeliveryHitMap::default();
    } else {
        let mut hit_map = crate::tui::workflow::hit_map::DeliveryHitMap::default();
        crate::tui::workflow::widget::draw_delivery_tab(
            f,
            delivery_area,
            &app.delivery_snapshot,
            &app.workflow_nav,
            &theme,
            app.tick_count,
            &mut hit_map,
        );
        hit_map.inspector = inspector_area;
        app.delivery_hit_map = hit_map;
    }

    let regions = crate::tui::workflow::regions::compute_regions(delivery_area);
    focus::register_pane(app, PaneId::WorkflowMissionStrip, regions.mission);
    focus::register_pane(app, PaneId::WorkflowPrRail, regions.pr_rail);
    focus::register_pane(app, PaneId::WorkflowPhaseRail, regions.phase_rail);
    focus::register_pane(app, PaneId::WorkflowCanvas, regions.canvas);
    focus::register_pane(app, PaneId::WorkflowMinimap, regions.minimap);
    if let Some(area) = inspector_area {
        focus::register_pane(app, PaneId::WorkflowInspector, area);
        focus::register_esc_hotspot(app, PaneId::WorkflowInspector, area);
    }

    if let Some(area) = inspector_area {
        let selected_id = app
            .workflow_nav
            .selected_node_id(&app.workflow_snapshot)
            .map(str::to_string);
        draw_inspector_pane(
            f,
            area,
            &app.delivery_snapshot,
            selected_id.as_deref(),
            app.inspector_tab,
            &app.state.live_log,
            app.delivery_action_message.as_deref(),
            &theme,
        );
    } else if app.workflow_inspect_open {
        // Narrow-terminal fallback: legacy modal overlay.
        draw_workflow_inspect_overlay(f, app);
    }
}
