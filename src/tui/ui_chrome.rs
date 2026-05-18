use super::*;

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

pub(crate) fn status_color(status: &str) -> Color {
    match status {
        "success" | "omitted" | "vti-skipped" => Color::Green,
        "running" => Color::Blue,
        "failed" => Color::Red,
        "pending" | "created" => Color::Yellow,
        "canceled" => Color::DarkGray,
        _ => Color::Gray,
    }
}

pub(crate) fn release_color(state: &str) -> Color {
    match state {
        "green" | "released" => Color::Green,
        "in-flight" | "canary-authorized" => Color::Cyan,
        "waiting" | "ready-for-canary" => Color::Yellow,
        "blocked" | "blocked-by-upstream" => Color::Magenta,
        "failed" => Color::Red,
        _ => Color::DarkGray,
    }
}

pub(crate) fn pane_border(pane: ActivePane, app: &App) -> Color {
    if app.active_pane == pane {
        Color::Cyan
    } else {
        Color::DarkGray
    }
}

pub(crate) fn status_badge(status: &str) -> (&'static str, Color) {
    match status {
        "success" | "passed" | "green" | "released" => ("PASS", Color::Green),
        "running" | "in-flight" | "canary-authorized" => ("RUN", Color::Cyan),
        "failed" => ("FAIL", Color::Red),
        "blocked" | "blocked-by-upstream" => ("BLOCK", Color::Magenta),
        "pending"
        | "created"
        | "waiting"
        | "waiting_for_resource"
        | "preparing"
        | "ready-for-canary" => ("WAIT", Color::Yellow),
        "canceled" | "vti-skipped" | "omitted" => ("SKIP", Color::DarkGray),
        _ => ("INFO", Color::Gray),
    }
}

pub(crate) fn meter_bar(percent: u16, width: usize) -> String {
    let width = width.max(1);
    let filled = (percent.min(100) as usize * width + 50) / 100;
    format!(
        "{}{} {:>3}%",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled)),
        percent.min(100)
    )
}

pub(crate) fn compact_spark(values: &[i64], width: usize) -> String {
    const STEPS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if values.is_empty() || width == 0 {
        return "n/a".to_string();
    }
    let take = width.min(values.len());
    let slice = &values[values.len() - take..];
    let min = slice.iter().copied().min().unwrap_or(0);
    let max = slice.iter().copied().max().unwrap_or(min);
    if max == min {
        return STEPS[0].to_string().repeat(take);
    }
    slice
        .iter()
        .map(|value| {
            let idx = (((*value - min) as f64 / (max - min) as f64) * 7.0).round() as usize;
            STEPS[idx.min(7)]
        })
        .collect()
}

pub(crate) fn top_attention(app: &App) -> (String, Color, String) {
    if app.state.active_taint_count > 0 {
        return (
            format!(
                "{} active cache taint(s) can block trusted proof reuse",
                app.state.active_taint_count
            ),
            Color::Magenta,
            "Open Cache, inspect taint scope, then run clean validation".to_string(),
        );
    }
    if let Some(rel) = &app.state.release_status
        && !matches!(rel.canary_state.as_str(), "green" | "released")
    {
        return (
            format!("Release {} is {}", rel.attempt.version, rel.canary_state),
            release_color(&rel.canary_state),
            "Open Release, inspect missing gate evidence".to_string(),
        );
    }
    if let Some(job) = app
        .state
        .recent_jobs
        .iter()
        .find(|job| job.status == "failed")
    {
        return (
            format!(
                "Job #{} failed in {}",
                job.job_id,
                job.job_name.as_deref().unwrap_or("unknown job")
            ),
            Color::Red,
            "Open evidence capsule or revisit after blocker explanation".to_string(),
        );
    }
    if app
        .state
        .recent_jobs
        .iter()
        .any(|job| job.status == "running")
    {
        return (
            "Validation is active on the critical path".to_string(),
            Color::Cyan,
            "Watch Flow Board and open the slowest running job".to_string(),
        );
    }
    if !app.state.gitlab_ready {
        return (
            "GitLab is not ready".to_string(),
            Color::Yellow,
            "Wait for service readiness or inspect docker status".to_string(),
        );
    }
    (
        "No blocking proof gaps detected".to_string(),
        Color::Green,
        "Start work, run VTI planning, or inspect latest release state".to_string(),
    )
}

/// Returns (outdated_age_secs, outdated_color, outdated_label) based on last_sync_at.
pub(crate) fn outdated_indicator(app: &App) -> (i64, Color, &'static str) {
    let age = app
        .state
        .last_sync_at
        .map(|t| chrono::Utc::now().signed_duration_since(t).num_seconds())
        .unwrap_or(0);
    if age < 5 {
        (age, Color::Green, "")
    } else if age < 30 {
        (age, Color::DarkGray, "[OUTDATED]")
    } else if age < 120 {
        (age, Color::Yellow, "[OUTDATED]")
    } else if age < 300 {
        (age, Color::LightRed, "[OUTDATED]")
    } else {
        (age, Color::Red, "!! DATA OUTDATED !!")
    }
}

// ---------------------------------------------------------------------------
// Header + Tab bar (2 rows merged into 1 widget)
// ---------------------------------------------------------------------------

pub(crate) fn draw_header_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    let (outdated_age, outdated_color, outdated_label) = outdated_indicator(app);

    let gitlab_span = if app.state.gitlab_ready {
        Span::styled("GitLab:OK", Style::default().fg(Color::Green))
    } else {
        Span::styled("GitLab:BOOT", Style::default().fg(Color::Yellow))
    };

    let pools_total = app.state.pools.len();
    let pools_active = app.state.pools.iter().filter(|p| !p.paused).count();

    let release_span = if let Some(ref rel) = app.state.release_status {
        let short_sha = rel.attempt.sha.get(..8).unwrap_or(rel.attempt.sha.as_str());
        Span::styled(
            format!(" rel:{} {}", short_sha, rel.canary_state),
            Style::default()
                .fg(release_color(&rel.canary_state))
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(" rel:none", Style::default().fg(Color::DarkGray))
    };

    let outdated_span = if !outdated_label.is_empty() {
        Span::styled(
            format!(" {}({}s)", outdated_label, outdated_age),
            Style::default()
                .fg(outdated_color)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let tab_defs: &[(&str, ActiveTab, u8)] = &[
        ("Workflow", ActiveTab::Workflow, 0),
        ("Mission", ActiveTab::Mission, 1),
        ("Release", ActiveTab::Release, 2),
        ("Approvals", ActiveTab::Approvals, 3),
        ("Jobs", ActiveTab::Jobs, 4),
        ("Agents", ActiveTab::Agents, 5),
        ("Tests", ActiveTab::Tests, 6),
        ("Pools", ActiveTab::Pools, 7),
        ("Cache", ActiveTab::Cache, 8),
        ("Evidence", ActiveTab::Evidence, 9),
    ];

    let top_spans: Vec<Span> = vec![
        Span::styled(
            " jeryu ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        gitlab_span,
        Span::styled(
            format!(" ctrs:{}", app.state.active_containers),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!(" pools:{}/{}", pools_active, pools_total),
            Style::default().fg(if pools_active == pools_total {
                Color::Green
            } else {
                Color::Yellow
            }),
        ),
        release_span,
        // v3 — Agent count badge
        Span::styled(
            format!(" agents:{}", app.state.agent_pipelines.len()),
            Style::default().fg(if app.state.agent_pipelines.is_empty() {
                Color::DarkGray
            } else {
                Color::Rgb(102, 255, 255)
            }),
        ),
        // v3 — Cache hit ratio
        Span::styled(
            format!(" cache:{:.0}%", app.state.hit_ratio * 100.0),
            Style::default().fg(if app.state.hit_ratio > 0.8 {
                Color::Green
            } else if app.state.hit_ratio > 0.5 {
                Color::Yellow
            } else {
                Color::Red
            }),
        ),
        // v3 — Taint indicator
        if app.state.active_taint_count > 0 {
            Span::styled(
                format!(" taint:{}", app.state.active_taint_count),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        },
        outdated_span,
        // Agent connection status: blinking green when connected, red when disconnected
        {
            let is_connected = app.state.agent_connected;
            let tick = app.tick_count;
            if is_connected {
                // blink at 0.5 Hz when connected
                let modifier = if (tick / 2).is_multiple_of(2) {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                };
                Span::styled(
                    " ●",
                    Style::default()
                        .fg(Color::Rgb(102, 204, 153))
                        .add_modifier(modifier),
                )
            } else {
                // static red when disconnected
                Span::styled(" ●", Style::default().fg(Color::Rgb(255, 102, 102)))
            }
        },
    ];

    let mut tab_spans: Vec<Span> = vec![];
    for (name, tab, n) in tab_defs {
        if app.active_tab == *tab {
            tab_spans.push(Span::styled(
                format!("[{}:{}]", n, name),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(
                format!(" {}:{} ", n, name),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    let p = Paragraph::new(vec![Line::from(top_spans), Line::from(tab_spans)])
        .block(Block::default().borders(Borders::BOTTOM))
        .style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}

#[path = "ui_chrome_footer.rs"]
mod ui_chrome_footer;
pub(crate) use ui_chrome_footer::*;
