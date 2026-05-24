use super::*;
pub(crate) fn draw_agents_tab(f: &mut Frame, app: &mut App, area: Rect) {
    // When the store exposes a real agent-sessions list, render the dedicated
    // agent-fleet cockpit widget. Until then, fall back to the pipeline-derived
    // view below with a banner explaining the data source.
    if !app.state.agent_sessions.is_empty() {
        let theme = crate::tui::theme::Theme::dark();
        focus::register_pane(app, PaneId::AgentsSessions, area);
        focus::register_drill_esc_hotspot(app, PaneId::AgentsSessions, area);
        crate::tui::widgets::agent_fleet::render_agent_fleet(
            f,
            area,
            &app.state.agent_sessions,
            app.selected_job_index,
            &theme,
        );
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(36),
            Constraint::Percentage(39),
            Constraint::Percentage(25),
        ])
        .split(area);

    focus::register_pane(app, PaneId::AgentsSessions, cols[0]);
    focus::register_drill_esc_hotspot(app, PaneId::AgentsSessions, cols[0]);

    let items: Vec<ListItem> = app
        .state
        .agent_pipelines
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let selected = i == app.selected_job_index;
            let prefix = if selected { ">>" } else { "  " };
            let short_sha = p.sha.get(..8).unwrap_or(&p.sha);
            let ts = p.updated_at.get(..16).unwrap_or(&p.updated_at);
            let (badge, color) = status_badge(&p.status);
            let phase = agent_phase_for_status(&p.status);
            let line = Line::from(vec![
                Span::styled(
                    format!("{prefix} {badge:<5} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<22} ", short_text(&p.ref_name, 22)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:<9} {} {}", phase, short_sha, ts),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let sessions_chrome = focus::pane_chrome(app, PaneId::AgentsSessions);
    let list = List::new(items).block(
        Block::default()
            .title(sessions_chrome.title(&format!(
                "Agent Sessions ({})",
                app.state.agent_pipelines.len()
            )))
            .borders(Borders::ALL)
            .border_style(sessions_chrome.border_style),
    );
    f.render_widget(list, cols[0]);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(13), Constraint::Min(8)])
        .split(cols[1]);

    focus::register_pane(app, PaneId::AgentsCockpit, rows[0]);
    focus::register_pane(app, PaneId::AgentsTimeline, rows[1]);
    focus::register_pane(app, PaneId::AgentsActions, cols[2]);
    focus::register_drill_esc_hotspot(app, PaneId::AgentsCockpit, rows[0]);
    focus::register_drill_esc_hotspot(app, PaneId::AgentsTimeline, rows[1]);
    focus::register_drill_esc_hotspot(app, PaneId::AgentsActions, cols[2]);

    let cockpit_chrome = focus::pane_chrome(app, PaneId::AgentsCockpit);
    let detail_block = Block::default()
        .title(cockpit_chrome.title("Agent Cockpit"))
        .borders(Borders::ALL)
        .border_style(cockpit_chrome.border_style);
    let detail_inner = detail_block.inner(rows[0]);
    f.render_widget(detail_block, rows[0]);

    if let Some(p) = app.state.agent_pipelines.get(app.selected_job_index) {
        let phase = agent_phase_for_status(&p.status);
        let progress = match p.status.as_str() {
            "success" => 100,
            "failed" => 100,
            "running" => 68,
            "pending" | "created" => 20,
            _ => 42,
        };
        let next_action = match p.status.as_str() {
            "failed" => "open evidence capsule or spawn repair",
            "running" => "watch pipeline logs and VTI receipt",
            "success" => "request merge proof dry-run",
            _ => "wait for runner assignment",
        };
        let lines = vec![
            Line::from(vec![
                Span::styled("Goal:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    short_text(&p.ref_name, 46),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Phase:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    phase,
                    Style::default()
                        .fg(status_color(&p.status))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  Progress ", Style::default().fg(Color::DarkGray)),
                Span::styled(meter_bar(progress, 10), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Branch:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&p.ref_name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("SHA:      ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    p.sha.get(..12).unwrap_or(&p.sha),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::styled("Status:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&p.status, Style::default().fg(status_color(&p.status))),
            ]),
            Line::from(vec![
                Span::styled("Pipeline: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("#{} (project #{})", p.pipeline_id, p.project_id),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("Updated:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(&p.updated_at, Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Next:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(next_action, Style::default().fg(Color::Yellow)),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), detail_inner);
    } else {
        f.render_widget(
            Paragraph::new(
                "\n  No agent sessions yet.\n  Branch names starting with agent/ appear here.",
            )
            .style(Style::default().fg(Color::DarkGray)),
            detail_inner,
        );
    }

    let timeline_chrome = focus::pane_chrome(app, PaneId::AgentsTimeline);
    let cap_block = Block::default()
        .title(timeline_chrome.title("Agent Timeline"))
        .borders(Borders::ALL)
        .border_style(timeline_chrome.border_style);
    let cap_inner = cap_block.inner(rows[1]);
    f.render_widget(cap_block, rows[1]);

    let cap_items: Vec<Line> = app
        .state
        .recent_audit_events
        .iter()
        .filter(|ev| {
            ev.event_type.contains("capability")
                || ev.event_type.contains("agent")
                || ev.event_type.contains("propose")
                || ev.event_type.contains("merge")
                || ev.event_type.contains("patch")
        })
        .take(cap_inner.height as usize)
        .map(|ev| {
            let ts = ev.timestamp.get(..16).unwrap_or(&ev.timestamp);
            let (badge, color) = if ev.event_type.contains("grant") {
                ("GRANT", Color::Yellow)
            } else if ev.event_type.contains("merge") {
                ("MERGE", Color::Magenta)
            } else if ev.event_type.contains("capability") {
                ("CAP", Color::Cyan)
            } else {
                ("STEP", Color::Green)
            };
            Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{badge:<6} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<24} ", short_text(&ev.event_type, 24)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("actor:{}", short_text(&ev.actor, 16)),
                    Style::default().fg(Color::White),
                ),
            ])
        })
        .collect();

    if cap_items.is_empty() {
        f.render_widget(
            Paragraph::new("  No agent/capability timeline events recorded.")
                .style(Style::default().fg(Color::DarkGray)),
            cap_inner,
        );
    } else {
        f.render_widget(Paragraph::new(cap_items), cap_inner);
    }

    draw_agent_actions(f, app, cols[2]);
}

// ---------------------------------------------------------------------------
// Tab 5 — Tests (existing)
// ---------------------------------------------------------------------------

pub(crate) fn draw_tests_tab(f: &mut Frame, app: &mut App, area: Rect) {
    // Optional VRC plan banner + Witness graph summary across the top.
    let vrc_visible = app.state.vrc.loaded && !app.state.vrc.plan.mode.is_empty();
    let witness_visible = app.state.witness.loaded && app.state.witness.crate_count > 0;
    let banner_height = match (vrc_visible, witness_visible) {
        (true, true) => 6,
        (true, false) | (false, true) => 3,
        (false, false) => 0,
    };
    let (vrc_area, witness_area, body_area) = if banner_height > 0 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(banner_height), Constraint::Min(8)])
            .split(area);
        let banner = rows[0];
        if vrc_visible && witness_visible {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(3)])
                .split(banner);
            (Some(split[0]), Some(split[1]), rows[1])
        } else if vrc_visible {
            (Some(banner), None, rows[1])
        } else {
            (None, Some(banner), rows[1])
        }
    } else {
        (None, None, area)
    };
    if let Some(banner) = vrc_area {
        draw_vrc_banner(f, app, banner);
    }
    if let Some(banner) = witness_area {
        draw_witness_summary(f, app, banner);
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(body_area);

    focus::register_pane(app, PaneId::TestsBottlenecks, chunks[0]);
    focus::register_pane(app, PaneId::TestsHistory, chunks[1]);
    focus::register_drill_esc_hotspot(app, PaneId::TestsBottlenecks, chunks[0]);
    focus::register_drill_esc_hotspot(app, PaneId::TestsHistory, chunks[1]);

    let (bottlenecks, label) = match app.test_view_mode {
        crate::tui::app::TestViewMode::Average => (&app.state.test_bottlenecks_avg, "Average"),
        crate::tui::app::TestViewMode::Latest => (&app.state.test_bottlenecks_latest, "Latest"),
    };

    let items: Vec<ListItem> = bottlenecks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let color = if i == app.selected_test_index {
                Color::Black
            } else if match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => b.avg_duration_ms > 300_000.0,
                crate::tui::app::TestViewMode::Latest => b.latest_duration_ms > 300_000,
            } {
                Color::Red
            } else if match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => b.avg_duration_ms > 60_000.0,
                crate::tui::app::TestViewMode::Latest => b.latest_duration_ms > 60_000,
            } {
                Color::Yellow
            } else {
                Color::Green
            };

            let bg = if i == app.selected_test_index {
                Color::Cyan
            } else {
                Color::Reset
            };

            let dur_text = match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => {
                    format!("{:.1}s", b.avg_duration_ms / 1000.0)
                }
                crate::tui::app::TestViewMode::Latest => {
                    format!("{:.1}s", b.latest_duration_ms as f64 / 1000.0)
                }
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<10} ", dur_text),
                    Style::default().fg(color).bg(bg),
                ),
                Span::styled(
                    format!("({:02}x) ", b.count),
                    Style::default().fg(Color::DarkGray).bg(bg),
                ),
                Span::styled(
                    b.test_name.clone(),
                    Style::default()
                        .fg(if i == app.selected_test_index {
                            Color::Black
                        } else {
                            Color::White
                        })
                        .bg(bg),
                ),
            ]))
        })
        .collect();

    let bottlenecks_chrome = focus::pane_chrome(app, PaneId::TestsBottlenecks);
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(bottlenecks_chrome.title(&format!("Bottlenecks ({}) - 'v' to toggle", label)))
            .border_style(bottlenecks_chrome.border_style),
    );
    f.render_widget(list, chunks[0]);

    let history_chrome = focus::pane_chrome(app, PaneId::TestsHistory);
    let history_block = Block::default()
        .borders(Borders::ALL)
        .title(history_chrome.title("History Drill-Down - Enter to load"))
        .border_style(history_chrome.border_style);

    if let Some(hist) = &app.selected_test_history {
        let h_items: Vec<ListItem> = hist
            .iter()
            .map(|h| {
                let color = match h.status.as_str() {
                    "success" | "passed" => Color::Green,
                    "failed" => Color::Red,
                    _ => Color::Yellow,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(
                            "{:<15} ",
                            h.created_at.split('T').next().unwrap_or(&h.created_at)
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(format!("{:<8} ", h.status), Style::default().fg(color)),
                    Span::styled(
                        format!("{:.1}s", h.duration_ms as f64 / 1000.0),
                        Style::default().fg(Color::White),
                    ),
                ]))
            })
            .collect();
        f.render_widget(List::new(h_items).block(history_block), chunks[1]);
    } else {
        f.render_widget(
            Paragraph::new("\n  Choose a test and press [Enter] to view execution history.")
                .block(history_block)
                .style(Style::default().fg(Color::DarkGray)),
            chunks[1],
        );
    }
}

fn draw_witness_summary(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.state.witness;
    let largest = w
        .largest_crate
        .as_ref()
        .map(|(name, n)| format!("largest: {name} ({n} items)"))
        .unwrap_or_else(|| "no pub_items".into());
    let spans = vec![
        Span::styled(
            "  Witness ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("crates:{}  pub_items:{}  ", w.crate_count, w.pub_item_count),
            Style::default().fg(Color::White),
        ),
        Span::styled(largest, Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("   at: {}", w.generated_at),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_vrc_banner(f: &mut Frame, app: &App, area: Rect) {
    let plan = &app.state.vrc.plan;
    let mode_color = match plan.mode.as_str() {
        "full" => Color::Cyan,
        "selected" => Color::Green,
        "none" => Color::DarkGray,
        _ => Color::Yellow,
    };
    let mut spans: Vec<Span> = vec![
        Span::styled(
            "  VRC plan ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("mode={} ", plan.mode),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {}", plan.reason),
            Style::default().fg(Color::Gray),
        ),
    ];
    if !plan.selected_tests.is_empty() || !plan.skipped_tests.is_empty() {
        spans.push(Span::styled(
            format!(
                "   selected:{}  skipped:{}",
                plan.selected_tests.len(),
                plan.skipped_tests.len()
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(conf) = plan.confidence {
        spans.push(Span::styled(
            format!("  conf:{:.0}%", conf * 100.0),
            Style::default().fg(Color::Cyan),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

// ---------------------------------------------------------------------------
// Tab 6 — Pools and Tab 7 — Cache + shared renderers
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_more_pools.rs"]
mod ui_panels_body_more_pools;
pub(crate) use ui_panels_body_more_pools::*;

#[path = "ui_panels_body_more_extra.rs"]
mod ui_panels_body_more_extra;
pub(crate) use ui_panels_body_more_extra::*;

#[path = "ui_panels_body_llms.rs"]
mod ui_panels_body_llms;
pub(crate) use ui_panels_body_llms::*;

#[path = "ui_panels_body_more_git.rs"]
mod ui_panels_body_more_git;
pub(crate) use ui_panels_body_more_git::*;

// ---------------------------------------------------------------------------
// Shared renderers (preserved from previous version)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[path = "ui_panels_body_tail.rs"]
mod ui_panels_body_tail;
pub(crate) use ui_panels_body_tail::*;
