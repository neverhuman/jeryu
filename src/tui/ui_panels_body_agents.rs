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
// Companion module wiring — Tests, Pools, Cache, Evidence, LLMs, Git, Secrets
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_tests.rs"]
mod ui_panels_body_tests;
pub(crate) use ui_panels_body_tests::*;

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
