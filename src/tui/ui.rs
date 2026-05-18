//! Owner: Interactive TUI subsystem — rendering logic
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform control-plane mutations directly.
//! v3: Integrated theme system, VTI badges, and contextual keybindings.
#[path = "ui_chrome.rs"]
mod ui_chrome;
#[path = "ui_panels.rs"]
mod ui_panels;
use super::app::{ActiveTab, App};
use crate::tui::{
    activity,
    focus::{self, PaneId},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
};
use ui_chrome::*;
use ui_panels::*;

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.focus.active.tab() != app.active_tab {
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
                Constraint::Length(3), // Header + tabs
                Constraint::Min(10),   // Full log view
                Constraint::Length(1), // Footer
            ])
            .split(f.area());

        draw_header_tabs(f, app, chunks[0]);
        activity::draw_activity_pane(f, app, chunks[1]);
        draw_footer(f, app, chunks[2]);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header + tabs
            Constraint::Min(10),   // Content
            Constraint::Length(7), // Activity / Logs
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    draw_header_tabs(f, app, chunks[0]);

    match app.active_tab {
        ActiveTab::Workflow => {
            // Refresh the Delivery snapshot and the mirrored workflow snapshot
            // for the legacy nav helpers.
            app.refresh_workflow_snapshot();
            let theme = crate::tui::theme::Theme::dark();

            use crate::tui::workflow::inspector::{
                INSPECTOR_MIN_TERM_W, INSPECTOR_W, draw_inspector_pane,
            };
            let main_area = chunks[1];
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
        ActiveTab::Mission => draw_mission_tab(f, app, chunks[1]),
        ActiveTab::Release => draw_release_tab(f, app, chunks[1]),
        ActiveTab::Approvals => draw_approvals_tab(f, app, chunks[1]),
        ActiveTab::Jobs => draw_jobs_tab(f, app, chunks[1]),
        ActiveTab::Agents => draw_agents_tab(f, app, chunks[1]),
        ActiveTab::Tests => draw_tests_tab(f, app, chunks[1]),
        ActiveTab::Pools => draw_pools_tab(f, app, chunks[1]),
        ActiveTab::Cache => draw_cache_dashboard(f, app, chunks[1]),
        ActiveTab::Evidence => draw_evidence_tab(f, app, chunks[1]),
        ActiveTab::LLMs => draw_llms_tab(f, app, chunks[1]),
        ActiveTab::Git => draw_git_tab(f, app, chunks[1]),
        ActiveTab::Secrets => draw_secrets_tab(f, app, chunks[1]),
    }

    activity::draw_activity_pane(f, app, chunks[2]);
    draw_footer(f, app, chunks[3]);

    if app.command_palette_open {
        draw_command_palette(f, app);
    }
    if app.help_overlay_open {
        draw_help_overlay(f, app);
    }
}

/// Draw a centered overlay with full detail for the selected workflow node.
fn draw_workflow_inspect_overlay(f: &mut Frame, app: &App) {
    let theme = crate::tui::theme::Theme::dark();
    let area = f.area();

    // Center a box covering ~60% of the screen.
    let overlay_w = (area.width * 3 / 5)
        .max(50)
        .min(area.width.saturating_sub(4));
    let overlay_h = (area.height * 3 / 5)
        .max(16)
        .min(area.height.saturating_sub(4));
    let ox = area.x + (area.width.saturating_sub(overlay_w)) / 2;
    let oy = area.y + (area.height.saturating_sub(overlay_h)) / 2;
    let overlay_area = Rect::new(ox, oy, overlay_w, overlay_h);

    f.render_widget(Clear, overlay_area);

    let selected_id = app.workflow_nav.selected_node_id(&app.workflow_snapshot);
    let node = selected_id.and_then(|id| app.workflow_snapshot.node(id));

    let mut lines = Vec::new();

    if let Some(node) = node {
        let status_color = match node.status {
            crate::tui::workflow::model::WorkflowStatus::Ran => theme.ok,
            crate::tui::workflow::model::WorkflowStatus::Running => theme.running,
            crate::tui::workflow::model::WorkflowStatus::Error => theme.fail,
            crate::tui::workflow::model::WorkflowStatus::Waiting => theme.waiting,
            crate::tui::workflow::model::WorkflowStatus::Blocked => theme.blocked,
            _ => theme.text_secondary,
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", node.status.glyph()),
                theme.bold(status_color),
            ),
            Span::styled(
                node.label.clone(),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        lines.push(Line::from(vec![
            Span::styled("  Status:   ", theme.muted()),
            Span::styled(node.status.label(), theme.bold(status_color)),
            if node.critical_path {
                Span::styled("  [CRITICAL PATH]", theme.bold(theme.fail))
            } else {
                Span::raw("")
            },
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Kind:     ", theme.muted()),
            Span::styled(node.kind.label(), theme.secondary()),
        ]));

        if let Some(cmd) = &node.command {
            lines.push(Line::from(vec![
                Span::styled("  Command:  ", theme.muted()),
                Span::styled(cmd.clone(), theme.primary()),
            ]));
        }

        if !node.deps.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Deps:     ", theme.muted()),
                Span::styled(node.deps.join(", "), theme.secondary()),
            ]));
        }

        if let Some(pct) = node.progress_pct {
            lines.push(Line::from(vec![
                Span::styled("  Progress: ", theme.muted()),
                Span::styled(format!("{}%", pct), theme.bold(status_color)),
            ]));
        }

        if let Some(eta) = node.eta_secs {
            lines.push(Line::from(vec![
                Span::styled("  ETA:      ", theme.muted()),
                Span::styled(format!("{}s", eta), theme.secondary()),
            ]));
        }

        if let Some(dur) = node.duration_secs {
            lines.push(Line::from(vec![
                Span::styled("  Duration: ", theme.muted()),
                Span::styled(format!("{:.1}s", dur), theme.secondary()),
            ]));
        }

        if let Some(ref vti) = node.vti_status {
            lines.push(Line::from(vec![
                Span::styled("  VTI:      ", theme.muted()),
                Span::styled(vti.badge().to_string(), theme.bold(theme.vti_fire)),
            ]));
        }

        if let Some(ref cache) = node.cache_verdict {
            lines.push(Line::from(vec![
                Span::styled("  Cache:    ", theme.muted()),
                Span::styled(cache.badge().to_string(), theme.bold(theme.ok)),
            ]));
        }

        if let Some(ref reason) = node.reason {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Reason:   ", theme.muted()),
                Span::styled(reason.clone(), theme.secondary()),
            ]));
        }

        if !node.tags.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  Tags:     ", theme.muted()),
                Span::styled(node.tags.join(", "), theme.secondary()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press Enter or Esc to close",
            theme.muted(),
        )));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No node selected",
            theme.muted(),
        )));
    }

    let title = match node {
        Some(n) => format!(" [ Inspect: {} ] ", n.id),
        None => " [ Inspect ] ".to_string(),
    };

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_accent)),
            )
            .wrap(Wrap { trim: false }),
        overlay_area,
    );
}
