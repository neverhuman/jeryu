//! Owner: Interactive TUI subsystem — top-level frame orchestrator
//! Proof: `cargo nextest run -p jeryu -- tui::ui::draw`
//! Invariants: Pure rendering. Reads `App` state, registers focus
//!             panes, dispatches to per-tab draw functions in
//!             `ui_panels::*`.

use crate::tui::app::{ActiveTab, App};
use crate::tui::{
    activity,
    focus::{self, PaneId},
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use super::overlay::draw_workflow_inspect_overlay;
// ui_chrome and ui_panels declare their helpers as pub(crate); they are
// re-exported wholesale via `super::*` so name resolution lands here
// without forcing visibility changes in those legacy files.
use super::{
    draw_agents_tab, draw_approvals_tab, draw_bugs_tab, draw_cache_dashboard, draw_command_palette,
    draw_evidence_tab, draw_footer, draw_git_tab, draw_header_tabs, draw_help_overlay,
    draw_jank_tab, draw_jobs_tab, draw_llms_tab, draw_mission_tab, draw_pools_tab,
    draw_release_tab, draw_secrets_tab, draw_tests_tab,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    if !app.focus.active.is_global() && app.focus.active.tab() != app.active_tab {
        app.maximize_logs = false;
        app.focus.set_tab(app.active_tab);
    }
    app.focus_map.clear_for_tab(app.active_tab);

    let fullscreen_activity =
        app.maximize_logs || app.focus.fullscreen == Some(PaneId::ActivityLog(app.active_tab));

    if fullscreen_activity {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(10),
                Constraint::Length(1),
            ])
            .split(f.area());

        draw_header_tabs(f, app, chunks[0]);
        crate::tui::repo_fleet_bar::draw_fleet_bar(f, app, chunks[1]);
        focus::register_pane(app, PaneId::FleetBar, chunks[1]);
        activity::draw_activity_pane(f, app, chunks[2]);
        draw_footer(f, app, chunks[3]);
        if app.repo_detail_open {
            crate::tui::repo_fleet_bar::draw_repo_detail_overlay(f, app);
        }
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(7),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header_tabs(f, app, chunks[0]);
    crate::tui::repo_fleet_bar::draw_fleet_bar(f, app, chunks[1]);
    focus::register_pane(app, PaneId::FleetBar, chunks[1]);

    match app.active_tab {
        ActiveTab::Workflow => draw_workflow_tab(f, app, chunks[2]),
        ActiveTab::Mission => draw_mission_tab(f, app, chunks[2]),
        ActiveTab::Release => draw_release_tab(f, app, chunks[2]),
        ActiveTab::Approvals => draw_approvals_tab(f, app, chunks[2]),
        ActiveTab::Jobs => draw_jobs_tab(f, app, chunks[2]),
        ActiveTab::Agents => draw_agents_tab(f, app, chunks[2]),
        ActiveTab::Tests => draw_tests_tab(f, app, chunks[2]),
        ActiveTab::Pools => draw_pools_tab(f, app, chunks[2]),
        ActiveTab::Cache => draw_cache_dashboard(f, app, chunks[2]),
        ActiveTab::Evidence => draw_evidence_tab(f, app, chunks[2]),
        ActiveTab::Repos => draw_repos_tab(f, app, chunks[2]),
        ActiveTab::Bugs => draw_bugs_tab(f, app, chunks[2]),
        ActiveTab::LLMs => draw_llms_tab(f, app, chunks[2]),
        ActiveTab::Git => draw_git_tab(f, app, chunks[2]),
        ActiveTab::Secrets => draw_secrets_tab(f, app, chunks[2]),
        ActiveTab::Jankurai => draw_jank_tab(f, app, chunks[2]),
    }

    activity::draw_activity_pane(f, app, chunks[3]);
    draw_footer(f, app, chunks[4]);

    if app.command_palette_open {
        draw_command_palette(f, app);
    }
    if app.help_overlay_open {
        draw_help_overlay(f, app);
    }
    if app.repo_detail_open {
        crate::tui::repo_fleet_bar::draw_repo_detail_overlay(f, app);
    }
}

fn draw_repos_tab(f: &mut Frame, app: &mut App, area: Rect) {
    focus::register_pane(app, PaneId::ReposLens, area);
    focus::register_drill_esc_hotspot(app, PaneId::ReposLens, area);
    let input = crate::tui::lenses::repos::ReposLensInput::from_fleet_snapshot(&app.state.fleet);
    crate::tui::lenses::repos::draw(f, &input, area);
}

fn draw_workflow_tab(f: &mut Frame, app: &mut App, area: Rect) {
    app.refresh_workflow_snapshot();
    let theme = crate::tui::theme::Theme::dark();

    use crate::tui::workflow::inspector::{
        INSPECTOR_MIN_TERM_W, INSPECTOR_W, draw_inspector_pane_with_chrome,
    };
    use crate::tui::workflow::widget::DeliveryChrome;
    let main_area = area;
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
        crate::tui::workflow::widget::draw_workflow_empty_state(
            f,
            delivery_area,
            &theme,
            &app.state.delivery_source_status,
        );
        app.delivery_hit_map = crate::tui::workflow::hit_map::DeliveryHitMap::default();
    } else {
        let mut hit_map = crate::tui::workflow::hit_map::DeliveryHitMap::default();
        let delivery_chrome = DeliveryChrome {
            mission: Some(focus::pane_chrome(app, PaneId::WorkflowMissionStrip)),
            pr_rail: Some(focus::pane_chrome(app, PaneId::WorkflowPrRail)),
            phase_rail: Some(focus::pane_chrome(app, PaneId::WorkflowPhaseRail)),
            canvas: Some(focus::pane_chrome(app, PaneId::WorkflowCanvas)),
            minimap: Some(focus::pane_chrome(app, PaneId::WorkflowMinimap)),
        };
        let repo_filter = app.repo_filter();
        crate::tui::workflow::widget::draw_delivery_tab_with_chrome(
            f,
            delivery_area,
            &app.delivery_snapshot,
            &app.workflow_nav,
            &theme,
            app.tick_count,
            &mut hit_map,
            delivery_chrome,
            repo_filter,
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
    focus::register_drill_esc_hotspot(app, PaneId::WorkflowMissionStrip, regions.mission);
    focus::register_drill_esc_hotspot(app, PaneId::WorkflowPrRail, regions.pr_rail);
    focus::register_drill_esc_hotspot(app, PaneId::WorkflowPhaseRail, regions.phase_rail);
    focus::register_drill_esc_hotspot(app, PaneId::WorkflowCanvas, regions.canvas);
    focus::register_drill_esc_hotspot(app, PaneId::WorkflowMinimap, regions.minimap);
    if let Some(area) = inspector_area {
        focus::register_pane(app, PaneId::WorkflowInspector, area);
        focus::register_drill_esc_hotspot(app, PaneId::WorkflowInspector, area);
    }

    if let Some(area) = inspector_area {
        let selected_id = app
            .workflow_nav
            .selected_node_id(&app.workflow_snapshot)
            .map(str::to_string);
        draw_inspector_pane_with_chrome(
            f,
            area,
            &app.delivery_snapshot,
            selected_id.as_deref(),
            app.inspector_tab,
            &app.state.live_log,
            app.delivery_action_message.as_deref(),
            &theme,
            Some(focus::pane_chrome(app, PaneId::WorkflowInspector)),
        );
    } else if app.workflow_inspect_open {
        draw_workflow_inspect_overlay(f, app);
    }
}
