use super::*;
pub(crate) fn draw_mission_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Min(10),
        ])
        .split(area);
    let headline_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(44), Constraint::Length(42)])
        .split(rows[0]);
    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(44),
            Constraint::Percentage(34),
            Constraint::Percentage(22),
        ])
        .split(rows[2]);

    focus::register_pane(app, PaneId::MissionTopSignal, headline_cols[0]);
    focus::register_pane(app, PaneId::MissionReadiness, headline_cols[1]);
    focus::register_pane(app, PaneId::MissionMetrics, rows[1]);
    focus::register_pane(app, PaneId::MissionAttention, body_cols[0]);
    focus::register_pane(app, PaneId::MissionProofLanes, body_cols[1]);
    focus::register_pane(app, PaneId::MissionActions, body_cols[2]);

    let pool_active = app.state.pools.iter().filter(|p| !p.paused).count();
    let pool_total = app.state.pools.len();
    let pool_sync_warning = app.state.pool_sync_error.is_some();
    let running_jobs = app
        .state
        .recent_jobs
        .iter()
        .filter(|j| crate::tui::live::is_live_job_status(j.status.as_str()))
        .count();
    let failed_jobs = app
        .state
        .recent_jobs
        .iter()
        .filter(|j| j.status == "failed")
        .count();
    let blocked_work = failed_jobs
        + usize::from(app.state.active_taint_count > 0)
        + usize::from(
            app.state
                .release_status
                .as_ref()
                .is_some_and(|rel| !matches!(rel.canary_state.as_str(), "green" | "released")),
        );
    let release_ready = app
        .state
        .release_status
        .as_ref()
        .is_some_and(|rel| matches!(rel.canary_state.as_str(), "green" | "released"));
    let release_progress = app
        .state
        .release_status
        .as_ref()
        .map(|rel| match rel.canary_state.as_str() {
            "released" => 100,
            "green" => 92,
            "in-flight" | "canary-authorized" => 70,
            "ready-for-canary" => 55,
            "waiting" => 35,
            "blocked" | "blocked-by-upstream" => 25,
            "failed" => 10,
            _ => 20,
        })
        .unwrap_or(0);
    let cache_trust = if app.state.active_taint_count == 0 {
        100
    } else {
        35
    };
    let autonomy_score = 100u16
        .saturating_sub((blocked_work as u16).saturating_mul(18))
        .saturating_sub(if !app.state.gitlab_ready { 22 } else { 0 })
        .saturating_sub(if app.state.proxy_healthy { 0 } else { 8 })
        .min(100);
    let (headline, headline_color, next_action) = top_attention(app);
    let (_, outdated_color, outdated_label) = outdated_indicator(app);
    let runners_readiness = if pool_sync_warning {
        format!("{pool_active}/{pool_total} cached")
    } else {
        format!("{pool_active}/{pool_total} active")
    };

    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "  TOP SIGNAL  ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(headline_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {}", short_text(&headline, 84)),
                    Style::default()
                        .fg(headline_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Next action: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    short_text(&next_action, 92),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Freshness: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    if outdated_label.is_empty() {
                        "fresh"
                    } else {
                        outdated_label
                    },
                    Style::default().fg(outdated_color),
                ),
                Span::styled("   Command: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "^K actions  Enter inspect  3 flow  4 agents  8 evidence",
                    Style::default().fg(Color::Cyan),
                ),
            ]),
        ])
        .block(
            Block::default()
                .title(" [ Mission Control ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::MissionTopSignal)),
        ),
        headline_cols[0],
    );

    f.render_widget(
        Paragraph::new(vec![
            readiness_line(
                "GitLab",
                if app.state.gitlab_ready {
                    "PASS online"
                } else {
                    "WAIT booting"
                },
                if app.state.gitlab_ready {
                    Color::Green
                } else {
                    Color::Yellow
                },
            ),
            readiness_line(
                "Runners",
                &runners_readiness,
                if pool_sync_warning {
                    Color::LightRed
                } else if pool_active == pool_total {
                    Color::Green
                } else {
                    Color::Yellow
                },
            ),
            readiness_line(
                "Gateway",
                &format!(
                    "proxy:{} registry:{}",
                    if app.state.proxy_healthy {
                        "PASS"
                    } else {
                        "FAIL"
                    },
                    if app.state.registry_healthy {
                        "PASS"
                    } else {
                        "FAIL"
                    }
                ),
                if app.state.proxy_healthy && app.state.registry_healthy {
                    Color::Green
                } else {
                    Color::Red
                },
            ),
            readiness_line(
                "Containers",
                &app.state.active_containers.to_string(),
                Color::Cyan,
            ),
        ])
        .block(
            Block::default()
                .title(" [ Readiness ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::MissionReadiness)),
        ),
        headline_cols[1],
    );

    let autonomy_value = format!("{}%", autonomy_score);
    let active_work_value = format!("{} jobs", app.state.recent_jobs.len());
    let active_work_detail = format!("{running_jobs} running / {failed_jobs} failed");
    let release_value = if release_ready { "ready" } else { "proofing" };
    let cache_value = format!("{} taints", app.state.active_taint_count);
    let feed_count = app.state.runner_feeds.len();
    let feed_running = app
        .state
        .runner_feeds
        .iter()
        .filter(|f| crate::tui::live::is_live_job_status(f.status.as_str()))
        .count();
    let feed_failed = app
        .state
        .runner_feeds
        .iter()
        .filter(|f| f.status == "failed")
        .count();
    crate::tui::widgets::mission_shared::render_metric_row(
        f,
        rows[1],
        &[
            crate::tui::widgets::mission_shared::MetricTile {
                title: "Autonomy",
                value: &autonomy_value,
                detail: Some(&meter_bar(autonomy_score, 12)),
                color: if autonomy_score >= 80 {
                    Color::Green
                } else if autonomy_score >= 55 {
                    Color::Yellow
                } else {
                    Color::Red
                },
            },
            crate::tui::widgets::mission_shared::MetricTile {
                title: "Active Work",
                value: &active_work_value,
                detail: Some(&active_work_detail),
                color: if failed_jobs > 0 {
                    Color::Red
                } else if running_jobs > 0 {
                    Color::Cyan
                } else {
                    Color::Green
                },
            },
            crate::tui::widgets::mission_shared::MetricTile {
                title: "Release",
                value: release_value,
                detail: Some(&meter_bar(release_progress, 12)),
                color: if release_ready {
                    Color::Green
                } else {
                    Color::Yellow
                },
            },
            crate::tui::widgets::mission_shared::MetricTile {
                title: "Cache Trust",
                value: &cache_value,
                detail: Some(&meter_bar(cache_trust, 12)),
                color: if app.state.active_taint_count > 0 {
                    Color::Magenta
                } else {
                    Color::Green
                },
            },
            crate::tui::widgets::mission_shared::MetricTile {
                title: "Live Runners",
                value: &format!("{} active", feed_count),
                detail: Some(&format!("{feed_running}▶ {feed_failed}✕")),
                color: if feed_failed > 0 {
                    Color::Red
                } else if feed_running > 0 {
                    Color::Cyan
                } else {
                    Color::DarkGray
                },
            },
        ],
    );

    draw_attention_queue(f, app, body_cols[0]);
    draw_proof_lanes(f, app, body_cols[1]);
    draw_action_stack(f, app, body_cols[2]);
}

#[path = "ui_panels_mission_extra.rs"]
mod ui_panels_mission_extra;
pub(crate) use ui_panels_mission_extra::*;

#[path = "ui_panels_body_approvals.rs"]
mod ui_panels_body_approvals;
pub(crate) use ui_panels_body_approvals::*;

#[path = "ui_panels_body_bugs.rs"]
mod ui_panels_body_bugs;
pub(crate) use ui_panels_body_bugs::*;
