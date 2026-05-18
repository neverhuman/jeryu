use super::*;
pub(crate) fn draw_release_banner(f: &mut Frame, app: &App, area: Rect) {
    let body = if let Some(ref release) = app.state.release_status {
        let attempt = &release.attempt;
        format!(
            " Version: {}  State: {} ({})  Upstream: {}  Prod: {} {:?}  Note: {}",
            attempt.version,
            release.canary_state,
            release.eligibility,
            attempt.upstream_status,
            attempt
                .production_pipeline_status
                .as_deref()
                .unwrap_or("not-triggered"),
            attempt.production_pipeline_id,
            attempt.canary_note.as_deref().unwrap_or("(none)")
        )
    } else {
        " No release attempts yet.  Waiting for the first green main pipeline.".to_string()
    };

    let color = if let Some(ref release) = app.state.release_status {
        release_color(&release.canary_state)
    } else {
        Color::DarkGray
    };

    let panel = Paragraph::new(body)
        .block(
            Block::default()
                .title(" [ Release Watch ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(panel, area);
}

#[allow(dead_code)]
pub(crate) fn draw_flow_board(f: &mut Frame, app: &App, area: Rect) {
    let (outdated_age, outdated_color, _outdated_label) = outdated_indicator(app);
    let flow_outdated = app.state.flow.outdated;
    let title = if flow_outdated {
        if let Some(last) = app.state.flow.last_non_empty_at {
            let age = chrono::Utc::now()
                .signed_duration_since(last)
                .num_seconds()
                .max(0);
            format!(" FLOW BOARD [outdated {}s] ", age)
        } else {
            format!(" FLOW BOARD [outdated {}s] ", outdated_age)
        }
    } else {
        " FLOW BOARD ".to_string()
    };
    let border_color = if flow_outdated {
        outdated_color
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if let Some(pipe_flow) = app.state.flow.active_pipelines.first() {
        let selected_job_id = app.selected_job().map(|job| job.job_id);
        let selected_id = selected_job_id.and_then(|job_id| {
            pipe_flow
                .graph
                .nodes
                .iter()
                .find(|node| node.job_id == Some(job_id))
                .map(|node| node.id)
        });
        let widget = crate::tui::flow::FlowGraphWidget::new(&pipe_flow.graph, selected_id);
        f.render_widget(widget, inner_area);
    } else {
        let msg = if let Some(last) = app.state.flow.last_non_empty_at {
            let age = chrono::Utc::now()
                .signed_duration_since(last)
                .num_seconds()
                .max(0);
            format!("No active pipelines  (last seen {}s ago)", age)
        } else {
            "Waiting for active pipelines...".to_string()
        };
        let p = Paragraph::new(msg).block(Block::default());
        f.render_widget(p, inner_area);
    }
}

#[allow(dead_code)]
pub(crate) fn draw_jobs(f: &mut Frame, app: &App, area: Rect) {
    let active = app.active_tab == ActiveTab::Jobs;
    let now = chrono::Utc::now();
    let items: Vec<ListItem> = app
        .state
        .recent_jobs
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let selected = active && i == app.selected_job_index;
            let color = status_color(&j.status);
            let icon = match j.status.as_str() {
                "success" => "OK",
                "running" => "RUN",
                "failed" => "FAIL",
                "pending" | "created" | "waiting_for_resource" | "preparing" => "WAIT",
                "canceled" => "STOP",
                _ => "JOB",
            };

            let prefix = if selected { "> " } else { "  " };
            let name = j.job_name.as_deref().unwrap_or("unknown_job");

            let (pct, pct_color) = match j.status.as_str() {
                "success" => (100u16, Color::Green),
                "failed" | "canceled" => {
                    let run_secs = j.queued_duration.unwrap_or(0.0) as u64;
                    let p = if run_secs > 0 {
                        ((run_secs as f64 / 120.0) * 100.0).min(99.0) as u16
                    } else {
                        0
                    };
                    (
                        p,
                        if j.status == "failed" {
                            Color::Red
                        } else {
                            Color::DarkGray
                        },
                    )
                }
                "running" | "pending" | "created" | "waiting_for_resource" | "preparing" => {
                    let elapsed =
                        if let Ok(st) = chrono::DateTime::parse_from_rfc3339(&j.received_at) {
                            now.signed_duration_since(st).num_seconds()
                        } else {
                            0
                        };
                    let p = ((elapsed as f64 / 120.0) * 100.0).min(99.0) as u16;
                    (
                        p,
                        if j.status == "running" {
                            Color::Cyan
                        } else {
                            Color::Yellow
                        },
                    )
                }
                _ => (0, Color::DarkGray),
            };

            let elapsed = chrono::DateTime::parse_from_rfc3339(&j.received_at)
                .ok()
                .map(|st| {
                    chrono::Utc::now()
                        .signed_duration_since(st)
                        .num_seconds()
                        .max(0)
                })
                .unwrap_or(0);
            let pipeline = j
                .pipeline_id
                .map(|id| format!("#{}", id))
                .unwrap_or("#?".to_string());

            let content = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("{:<4} ", icon),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:>3}% ", pct), Style::default().fg(pct_color)),
                Span::styled(
                    format!("{:<6} ", format_duration(elapsed)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{:<6} ", pipeline),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(name.to_string(), Style::default().fg(Color::White)),
            ]);

            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(
                " [{}] Live Jobs ({}) ",
                if active { "*" } else { " " },
                app.state.recent_jobs.len()
            ))
            .borders(Borders::ALL)
            .border_style(focus::border_style(app, PaneId::JobsRunnerFeed)),
    );
    f.render_widget(list, area);
}

pub(crate) fn draw_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let job_name = if let Some(j) = app.selected_job() {
        match j.job_name.clone() {
            Some(value) => value,
            None => format!("Job #{}", j.job_id),
        }
    } else {
        "None".to_string()
    };
    let log_state = &app.state.live_log;
    let title_state = if let Some(error) = &log_state.error {
        format!("outdated: {}", short_text(error, 48))
    } else if log_state.outdated {
        "outdated".to_string()
    } else if log_state.target.is_some() {
        "live".to_string()
    } else {
        "idle".to_string()
    };
    let follow_state = if app.follow_log_tail {
        "follow"
    } else {
        "manual"
    };

    let outer_block = Block::default()
        .title(format!(
            " Log: {} [{} | {}] ",
            job_name, title_state, follow_state
        ))
        .borders(Borders::ALL)
        .border_style(focus::border_style(app, PaneId::JobsInspector));

    let inner_area = outer_block.inner(area);
    f.render_widget(outer_block, area);

    let (gauge_area, log_area) = if app.state.recent_jobs.is_empty() {
        (None, inner_area)
    } else {
        let rc = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(2)])
            .split(inner_area);
        (Some(rc[0]), rc[1])
    };

    if let Some(j) = app.selected_job()
        && let Some(g_area) = gauge_area
    {
        let mut elapsed = 0;
        if let Ok(st) = chrono::DateTime::parse_from_rfc3339(&j.received_at) {
            elapsed = chrono::Utc::now().signed_duration_since(st).num_seconds();
        }
        let pct = match j.status.as_str() {
            "success" | "failed" | "canceled" => 100,
            _ => {
                let mut p = (elapsed as f64 / 120.0 * 100.0) as u16;
                if p > 99 {
                    p = 99;
                }
                p
            }
        };
        let eta_str = if pct == 100 {
            "Done".to_string()
        } else {
            let eta = 120 - elapsed;
            if eta < 0 {
                "Finishing...".to_string()
            } else {
                format!("{}s", eta)
            }
        };

        let color = match j.status.as_str() {
            "failed" => Color::Red,
            "success" => Color::Green,
            _ => Color::Cyan,
        };

        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
            .percent(pct)
            .label(format!("{}% ({})", pct, eta_str));
        f.render_widget(gauge, g_area);
    }

    let parsed_text = if !log_state.text.is_empty() {
        render_log_text(&log_state.text)
    } else if app.active_tab == ActiveTab::Jobs {
        Text::raw("Choose a live, failed, or recent job. Fetching...")
    } else {
        Text::raw("Focus Jobs pane to tail logs...")
    };

    let total_lines = parsed_text.lines.len() as u16;
    let view_height = log_area.height;
    let max_scroll = total_lines.saturating_sub(view_height);
    if app.follow_log_tail || app.log_scroll_offset > max_scroll {
        app.log_scroll_offset = max_scroll;
    }

    let p = Paragraph::new(parsed_text)
        .wrap(Wrap { trim: false })
        .scroll((app.log_scroll_offset, 0));
    f.render_widget(p, log_area);
}

#[path = "ui_panels_body_tail_extra.rs"]
mod ui_panels_body_tail_extra;
pub(crate) use ui_panels_body_tail_extra::*;
