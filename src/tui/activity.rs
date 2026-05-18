use crate::tui::{
    app::{ActiveTab, App},
    focus::{self, PaneId},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn draw_activity_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let pane = PaneId::ActivityLog(app.active_tab);
    focus::register_pane(app, pane, area);
    focus::register_esc_hotspot(app, pane, area);

    if area.width == 0 || area.height == 0 {
        return;
    }

    let title = format!(
        " Activity / Logs{} ",
        focus::esc_label(focus::is_active(app, pane))
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(focus::border_style(app, pane));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text = activity_text(app, inner.height);
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White))
        .scroll((app.log_scroll_offset, 0));
    f.render_widget(paragraph, inner);
}

fn activity_text(app: &App, _height: u16) -> Text<'static> {
    let mut lines = Vec::new();
    match app.active_tab {
        ActiveTab::Workflow => {
            lines.extend(workflow_activity(app));
        }
        ActiveTab::Mission => lines.extend(mission_activity(app)),
        ActiveTab::Release => lines.extend(release_activity(app)),
        ActiveTab::Approvals => lines.extend(approvals_activity(app)),
        ActiveTab::Jobs => lines.extend(jobs_activity(app)),
        ActiveTab::Agents => lines.extend(agents_activity(app)),
        ActiveTab::Tests => lines.extend(tests_activity(app)),
        ActiveTab::Pools => lines.extend(pools_activity(app)),
        ActiveTab::Cache => lines.extend(cache_activity(app)),
        ActiveTab::Evidence => lines.extend(evidence_activity(app)),
        ActiveTab::Secrets => lines.extend(secrets_activity(app)),
        ActiveTab::LLMs => lines.extend(llms_activity(app)),
        ActiveTab::Git => lines.extend(git_activity(app)),
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " No activity yet.",
            Style::default().fg(Color::DarkGray),
        )));
    }
    Text::from(lines)
}

fn workflow_activity(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(pr) = app.delivery_snapshot.selected() {
        lines.push(row("Selected PR", &format!("#{}", pr.number), Color::Cyan));
        if let Some(node_id) = app.workflow_nav.selected_node_id(&app.workflow_snapshot)
            && let Some(node) = app.workflow_snapshot.node(node_id)
        {
            lines.push(row("Node", &node.label, Color::White));
            lines.push(row(
                "State",
                node.status.label(),
                status_color(node.status.label()),
            ));
        }
    }
    if let Some(msg) = &app.delivery_action_message {
        lines.push(row("Action", msg, Color::Yellow));
    }
    if let Some(log) = app.state.live_log.target {
        lines.push(row(
            "Log target",
            &format!("job#{}", log.job_id),
            Color::Green,
        ));
    }
    append_log_tail(&mut lines, &app.state.live_log.text, 6);
    lines
}

fn mission_activity(app: &App) -> Vec<Line<'static>> {
    let (headline, color, next_action) = top_attention(app);
    vec![
        row("Top signal", &headline, color),
        row("Next", &next_action, Color::White),
        row(
            "Jobs",
            &format!(
                "{} running / {} failed",
                app.state
                    .recent_jobs
                    .iter()
                    .filter(|job| job.status == "running")
                    .count(),
                app.state
                    .recent_jobs
                    .iter()
                    .filter(|job| job.status == "failed")
                    .count()
            ),
            Color::Cyan,
        ),
    ]
}

fn release_activity(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(rel) = &app.state.release_status {
        lines.push(row(
            "Release",
            &format!("{} / {}", rel.attempt.version, rel.canary_state),
            release_color(&rel.canary_state),
        ));
        lines.push(row("Ref", rel.attempt.ref_name.as_str(), Color::White));
        if let Some(note) = &rel.attempt.canary_note {
            lines.push(row("Note", note, Color::DarkGray));
        }
    } else {
        lines.push(row("Release", "none", Color::DarkGray));
    }
    lines
}

fn approvals_activity(app: &App) -> Vec<Line<'static>> {
    if let Some(approval) = app.state.approvals_queue.get(app.selected_approval_index) {
        vec![
            row("PR", &format!("#{}", approval.pr_number), Color::Cyan),
            row("Agent", &approval.agent_id, Color::White),
            row("CI", &approval.ci_status, Color::Green),
        ]
    } else {
        vec![row("Approvals", "queue empty", Color::DarkGray)]
    }
}

fn jobs_activity(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(job) = app.selected_job() {
        lines.push(row(
            "Job",
            &format!(
                "#{} {}",
                job.job_id,
                job.job_name.as_deref().unwrap_or("job")
            ),
            Color::Cyan,
        ));
        lines.push(row("Status", &job.status, status_color(&job.status)));
    }
    append_log_tail(&mut lines, &app.state.live_log.text, 10);
    if lines.is_empty() {
        lines.push(row("Jobs", "no job selected", Color::DarkGray));
    }
    lines
}

fn agents_activity(app: &App) -> Vec<Line<'static>> {
    if let Some(agent) = app.state.agent_pipelines.get(app.selected_job_index) {
        vec![
            row("Agent", &agent.ref_name, Color::Cyan),
            row("Status", &agent.status, status_color(&agent.status)),
            row(
                "Pipeline",
                &format!("#{} / #{}", agent.pipeline_id, agent.project_id),
                Color::White,
            ),
        ]
    } else {
        vec![row("Agents", "no sessions", Color::DarkGray)]
    }
}

fn tests_activity(app: &App) -> Vec<Line<'static>> {
    let (bottlenecks, label) = match app.test_view_mode {
        crate::tui::app::TestViewMode::Average => (&app.state.test_bottlenecks_avg, "average"),
        crate::tui::app::TestViewMode::Latest => (&app.state.test_bottlenecks_latest, "latest"),
    };
    if let Some(test) = bottlenecks.get(app.selected_test_index) {
        vec![
            row("View", label, Color::Cyan),
            row("Test", &test.test_name, Color::White),
            row("Count", &test.count.to_string(), Color::DarkGray),
        ]
    } else {
        vec![row("Tests", "no bottlenecks", Color::DarkGray)]
    }
}

fn pools_activity(app: &App) -> Vec<Line<'static>> {
    if let Some(pool) = app.state.pools.get(app.selected_pool_index) {
        vec![
            row("Pool", &pool.name, Color::Cyan),
            row(
                "Paused",
                if pool.paused { "yes" } else { "no" },
                Color::White,
            ),
        ]
    } else {
        vec![row("Pools", "empty", Color::DarkGray)]
    }
}

fn cache_activity(app: &App) -> Vec<Line<'static>> {
    vec![
        row(
            "Disk",
            &format!(
                "{}/{} GiB",
                app.state.storage_breakdown.disk_available_bytes / 1024 / 1024 / 1024,
                app.state.storage_breakdown.total_disk_bytes / 1024 / 1024 / 1024
            ),
            Color::Cyan,
        ),
        row(
            "Taints",
            &format!("{}", app.state.active_taint_count),
            if app.state.active_taint_count > 0 {
                Color::Magenta
            } else {
                Color::Green
            },
        ),
    ]
}

fn evidence_activity(app: &App) -> Vec<Line<'static>> {
    if let Some(rec) = app.state.recent_evidence.get(app.selected_evidence_index) {
        vec![
            row("Job", &format!("#{}", rec.job_id), Color::Cyan),
            row("Kind", &rec.failure_kind, Color::White),
            row("Stage", &rec.stage, Color::DarkGray),
        ]
    } else {
        vec![row("Evidence", "no capsules", Color::DarkGray)]
    }
}

fn secrets_activity(app: &App) -> Vec<Line<'static>> {
    if let Some(event) = app.state.secret_audit_events.get(app.selected_secret_index) {
        vec![
            row("Action", &event.action, Color::Cyan),
            row("Status", &event.status, Color::White),
            row("Repo", &event.repo_name, Color::DarkGray),
        ]
    } else {
        vec![row("Secrets", "no events", Color::DarkGray)]
    }
}

fn llms_activity(app: &App) -> Vec<Line<'static>> {
    vec![
        row(
            "Policy",
            &app.autonomy_dir
                .join("providers")
                .join("llm.yml")
                .display()
                .to_string(),
            Color::Cyan,
        ),
        row(
            "Resolved",
            if app.llm_secret_resolver.is_some() {
                "configured"
            } else {
                "env"
            },
            Color::White,
        ),
    ]
}

fn git_activity(app: &App) -> Vec<Line<'static>> {
    if let Some(event) = app.state.recent_git_events.get(app.selected_git_index) {
        vec![
            row("Command", &event.command_class, Color::Cyan),
            row(
                "Status",
                if event.exit_code == 0 {
                    "success"
                } else {
                    "failed"
                },
                Color::White,
            ),
            row("Args", &event.argv_redacted, Color::DarkGray),
        ]
    } else {
        vec![row("Git", "no commands", Color::DarkGray)]
    }
}

fn append_log_tail(lines: &mut Vec<Line<'static>>, text: &str, limit: usize) {
    for line in text.lines().take(limit) {
        lines.push(row("Log", line, Color::DarkGray));
    }
}

fn row(label: &str, value: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<10}"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(color)),
    ])
}

fn status_color(status: &str) -> Color {
    match status {
        "success" | "omitted" | "vti-skipped" => Color::Green,
        "running" => Color::Blue,
        "failed" => Color::Red,
        "pending" | "created" => Color::Yellow,
        "canceled" => Color::DarkGray,
        _ => Color::Gray,
    }
}

fn release_color(state: &str) -> Color {
    match state {
        "green" | "released" => Color::Green,
        "in-flight" | "canary-authorized" => Color::Cyan,
        "waiting" | "ready-for-canary" => Color::Yellow,
        "blocked" | "blocked-by-upstream" => Color::Magenta,
        "failed" => Color::Red,
        _ => Color::DarkGray,
    }
}

fn top_attention(app: &App) -> (String, Color, String) {
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
