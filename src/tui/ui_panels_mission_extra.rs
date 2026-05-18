use super::*;

pub(crate) fn readiness_line(label: &str, value: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label:<11}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(value.to_string(), Style::default().fg(color)),
    ])
}

#[allow(dead_code)] // kept for re-introduction in upcoming mission-redesign work
pub(crate) fn draw_metric_tile(
    f: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    detail: &str,
    color: Color,
) {
    crate::tui::widgets::mission_shared::render_metric_tile(
        f,
        area,
        title,
        value,
        Some(detail),
        color,
    );
}

pub(crate) fn draw_attention_queue(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    for job in app
        .state
        .recent_jobs
        .iter()
        .filter(|job| job.status == "failed")
        .take(4)
    {
        lines.push(attention_line(
            "P0",
            Color::Red,
            &format!("Job #{} failed", job.job_id),
            job.job_name.as_deref().unwrap_or("open logs/evidence"),
        ));
    }
    if app.state.active_taint_count > 0 {
        lines.push(attention_line(
            "P0",
            Color::Magenta,
            "Cache taint active",
            "trusted proof reuse blocked",
        ));
    }
    if let Some(rel) = &app.state.release_status
        && !matches!(rel.canary_state.as_str(), "green" | "released")
    {
        lines.push(attention_line(
            "P1",
            release_color(&rel.canary_state),
            &format!("Release {}", rel.canary_state),
            &rel.eligibility,
        ));
    }
    for job in app
        .state
        .recent_jobs
        .iter()
        .filter(|job| job.status == "running")
        .take(3)
    {
        lines.push(attention_line(
            "P2",
            Color::Cyan,
            &format!("Job #{} running", job.job_id),
            job.job_name.as_deref().unwrap_or("validation"),
        ));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No urgent blockers. Start with VTI planning or inspect latest release.",
            Style::default().fg(Color::Green),
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ Attention Queue ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

pub(crate) fn attention_line(
    priority: &str,
    color: Color,
    title: &str,
    detail: &str,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {priority:<3} "),
            Style::default()
                .fg(Color::Black)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:<28}", short_text(title, 28)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(short_text(detail, 44), Style::default().fg(Color::White)),
    ])
}

pub(crate) fn draw_proof_lanes(f: &mut Frame, app: &App, area: Rect) {
    let release_state = app
        .state
        .release_status
        .as_ref()
        .map(|rel| rel.canary_state.as_str())
        .unwrap_or("none");
    let lanes = vec![
        (
            "Capability grants",
            if app
                .state
                .recent_audit_events
                .iter()
                .any(|ev| ev.event_type.contains("capability"))
            {
                "observed"
            } else {
                "quiet"
            },
        ),
        (
            "VTI receipts",
            if app
                .state
                .recent_audit_events
                .iter()
                .any(|ev| ev.event_type.contains("vti"))
            {
                "observed"
            } else {
                "needed"
            },
        ),
        (
            "Merge proof",
            if failed_or_tainted(app) {
                "blocked"
            } else {
                "dry-run"
            },
        ),
        ("Release gate", release_state),
        ("Sandbox", "strict fails closed"),
        (
            "Evidence ledger",
            if app.state.recent_evidence.is_empty() {
                "empty"
            } else {
                "capsules"
            },
        ),
    ];
    let lines: Vec<Line> = lanes
        .into_iter()
        .map(|(lane, state)| {
            let (badge, color) = status_badge(state);
            Line::from(vec![
                Span::styled(
                    format!(" {badge:<5} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{lane:<18}"), Style::default().fg(Color::White)),
                Span::styled(state.to_string(), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ Proof Stack ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

pub(crate) fn draw_action_stack(f: &mut Frame, app: &App, area: Rect) {
    let jobs_by_state = [
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "running")
            .count() as i64,
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "pending" || j.status == "created")
            .count() as i64,
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "success")
            .count() as i64,
        app.state
            .recent_jobs
            .iter()
            .filter(|j| j.status == "failed")
            .count() as i64,
    ];
    let lines = vec![
        Line::from(vec![
            Span::styled("  CI shape   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                compact_spark(&jobs_by_state, 8),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Agents     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.state.agent_pipelines.len().to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Evidence   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.state.recent_evidence.len().to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Recommended",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ^K explain blockers",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  3 open flow board",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  4 inspect agents",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  8 open evidence",
            Style::default().fg(Color::White),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ Next Actions ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

pub(crate) fn failed_or_tainted(app: &App) -> bool {
    app.state.active_taint_count > 0
        || app
            .state
            .recent_jobs
            .iter()
            .any(|job| job.status == "failed")
}

// ---------------------------------------------------------------------------
// Tab 2 — Release: full gate matrix
// ---------------------------------------------------------------------------

#[path = "ui_panels_body.rs"]
mod ui_panels_body;
pub(crate) use ui_panels_body::*;
