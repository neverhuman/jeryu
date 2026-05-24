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
        Paragraph::new(lines).block({
            let chrome = focus::pane_chrome(app, PaneId::MissionAttention);
            Block::default()
                .title(chrome.title("Attention Queue"))
                .borders(Borders::ALL)
                .border_style(chrome.border_style)
        }),
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
    // Sandbox status derived from real cache + detonation counters instead
    // of the previous hardcoded label.
    let sandbox_state = if app.state.detonation_breaches > 0 {
        "breach"
    } else if app.state.active_taint_count > 0 {
        "taint active"
    } else {
        "strict closed"
    };
    // Build a string-owned lane list. When `proof-lanes.toml` is loaded,
    // surface those lanes with the derived live status; otherwise fall back
    // to the legacy heuristic set above.
    let mut lanes: Vec<(String, String)> = Vec::new();
    if app.state.proof_lanes.loaded {
        for lane in &app.state.proof_lanes.lanes {
            let id = lane.id.as_str();
            let status: &str = if app
                .state
                .recent_audit_events
                .iter()
                .any(|ev| ev.event_type.contains(id))
            {
                "observed"
            } else if lane.required_for.iter().any(|tag| tag == "any") {
                "needed"
            } else {
                "scoped"
            };
            // Capitalize the lane id for the label.
            let title = if let Some(first) = id.chars().next() {
                let mut s = first.to_ascii_uppercase().to_string();
                s.push_str(&id[first.len_utf8()..]);
                s
            } else {
                id.to_string()
            };
            lanes.push((title, status.to_string()));
        }
    }
    if lanes.is_empty() {
        // Legacy fallback when no proof-lanes.toml is present.
        lanes.push((
            "Capability grants".into(),
            if app
                .state
                .recent_audit_events
                .iter()
                .any(|ev| ev.event_type.contains("capability"))
            {
                "observed".into()
            } else {
                "quiet".into()
            },
        ));
        lanes.push((
            "VTI receipts".into(),
            if app
                .state
                .recent_audit_events
                .iter()
                .any(|ev| ev.event_type.contains("vti"))
            {
                "observed".into()
            } else {
                "needed".into()
            },
        ));
        lanes.push((
            "Merge proof".into(),
            if failed_or_tainted(app) {
                "blocked".into()
            } else {
                "dry-run".into()
            },
        ));
        lanes.push(("Release gate".into(), release_state.into()));
        lanes.push(("Sandbox".into(), sandbox_state.into()));
        lanes.push((
            "Evidence ledger".into(),
            if app.state.recent_evidence.is_empty() {
                "empty".into()
            } else {
                "capsules".into()
            },
        ));
    } else {
        // Append release + sandbox + evidence as derived signals not in toml.
        lanes.push(("Release gate".into(), release_state.into()));
        lanes.push(("Sandbox".into(), sandbox_state.into()));
        lanes.push((
            "Evidence ledger".into(),
            if app.state.recent_evidence.is_empty() {
                "empty".into()
            } else {
                "capsules".into()
            },
        ));
    }
    let lines: Vec<Line> = lanes
        .into_iter()
        .map(|(lane, state)| {
            let (badge, color) = status_badge(state.as_str());
            Line::from(vec![
                Span::styled(
                    format!(" {badge:<5} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{lane:<18}"), Style::default().fg(Color::White)),
                Span::styled(state, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block({
            let chrome = focus::pane_chrome(app, PaneId::MissionProofLanes);
            Block::default()
                .title(chrome.title("Proof Stack"))
                .borders(Borders::ALL)
                .border_style(chrome.border_style)
        }),
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
        Paragraph::new(lines).block({
            let chrome = focus::pane_chrome(app, PaneId::MissionActions);
            Block::default()
                .title(chrome.title("Next Actions"))
                .borders(Borders::ALL)
                .border_style(chrome.border_style)
        }),
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
