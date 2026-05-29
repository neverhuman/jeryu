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
    let (headline, headline_color, next_action) = top_attention(app);

    let gitlab_span = if app.state.gitlab_ready {
        Span::styled("GitLab:OK", Style::default().fg(Color::Green))
    } else {
        Span::styled("GitLab:BOOT", Style::default().fg(Color::Yellow))
    };

    let source_label = if outdated_label.is_empty() {
        "source:LIVE".to_string()
    } else {
        format!("source:{}:{}s", outdated_label, outdated_age)
    };
    let proof_label = if app.state.active_taint_count > 0 {
        "proof:TAINTED"
    } else if app.state.pool_sync_error.is_some() {
        "proof:PARTIAL"
    } else if app.state.gitlab_ready {
        "proof:LIVE"
    } else {
        "proof:SOURCE DOWN"
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
            " JERYU FLIGHT DECK ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        repo_filter_span,
        Span::raw(" "),
        gitlab_span,
        Span::styled(
            format!(" {source_label}"),
            Style::default()
                .fg(if outdated_label.is_empty() {
                    Color::Green
                } else {
                    outdated_color
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {proof_label}"),
            Style::default()
                .fg(match proof_label {
                    "proof:LIVE" => Color::Green,
                    "proof:PARTIAL" => Color::Yellow,
                    "proof:TAINTED" => Color::Magenta,
                    _ => Color::Red,
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" top:{}", compact_header_text(&headline, 24)),
            Style::default()
                .fg(headline_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" next:{}", compact_header_text(&next_action, 24)),
            Style::default().fg(Color::White),
        ),
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

fn compact_header_text(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let mut out: String = input.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[path = "ui_chrome_footer.rs"]
mod ui_chrome_footer;
pub(crate) use ui_chrome_footer::*;
