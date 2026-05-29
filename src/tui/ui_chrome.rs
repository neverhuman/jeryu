use super::*;

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

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
    let pools_using_cache = app.state.pool_sync_error.is_some();

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

    let tab_defs: &[(&str, ActiveTab, Option<u8>)] = &[
        ("Workflow", ActiveTab::Workflow, Some(0)),
        ("Mission", ActiveTab::Mission, Some(1)),
        ("Release", ActiveTab::Release, Some(2)),
        ("Approvals", ActiveTab::Approvals, Some(3)),
        ("Jobs", ActiveTab::Jobs, Some(4)),
        ("Agents", ActiveTab::Agents, Some(5)),
        ("Tests", ActiveTab::Tests, Some(6)),
        ("Pools", ActiveTab::Pools, Some(7)),
        ("Cache", ActiveTab::Cache, Some(8)),
        ("Evidence", ActiveTab::Evidence, Some(9)),
        ("Repos", ActiveTab::Repos, None),
        ("Bugs", ActiveTab::Bugs, None),
        ("Secrets", ActiveTab::Secrets, None),
        ("LLMs", ActiveTab::LLMs, None),
        ("Git", ActiveTab::Git, None),
        ("Jankurai", ActiveTab::Jankurai, None),
    ];

    let repo_filter = app.repo_filter();
    let repo_filter_span = match repo_filter {
        crate::repo_fleet::RepoFilter::All => {
            let n = app.state.fleet.repos.len();
            if n == 0 {
                Span::styled(" repo:All ", Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(
                    format!(" repo:All({}) ", n),
                    Style::default().fg(Color::Cyan),
                )
            }
        }
        crate::repo_fleet::RepoFilter::Only { alias, .. } => Span::styled(
            format!(" repo:{} ", alias),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let top_spans: Vec<Span> = vec![
        Span::styled(
            " jeryu ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        repo_filter_span,
        Span::raw(" "),
        gitlab_span,
        Span::styled(
            format!(" ctrs:{}", app.state.active_containers),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            if pools_using_cache {
                format!(" pools:{}/{} cached", pools_active, pools_total)
            } else {
                format!(" pools:{}/{}", pools_active, pools_total)
            },
            Style::default().fg(if pools_using_cache {
                Color::LightRed
            } else if pools_active == pools_total {
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
        let label = n
            .map(|n| format!("{n}:{name}"))
            .unwrap_or_else(|| name.to_string());
        if app.active_tab == *tab {
            tab_spans.push(Span::styled(
                format!("[{label}]"),
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(
                format!(" {label} "),
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
