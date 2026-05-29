use super::*;

pub(super) fn draw_release_evidence_pane(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(34)])
        .split(area);

    let _gate_color = match app.state.release_status.as_ref() {
        Some(r) => release_color(&r.canary_state),
        None => Color::DarkGray,
    };
    let gate_block = focus::pane_block(app, PaneId::ReleaseInspector, " [ Release Gate Matrix ] ");
    let gate_inner = gate_block.inner(cols[0]);
    f.render_widget(gate_block, cols[0]);

    if let Some(ref rel) = app.state.release_status {
        let attempt = &rel.attempt;
        let release_identity_detail = format!(
            "upstream={:?} release={:?} prod={:?}",
            attempt.upstream_pipeline_id,
            attempt.release_pipeline_id,
            attempt.production_pipeline_id
        );

        let gate_rows: Vec<(&str, &str, &str, &str)> = vec![
            (
                "Main branch green",
                if attempt.upstream_status == "success" {
                    "[OK]"
                } else {
                    "[WAIT]"
                },
                attempt.upstream_status.as_str(),
                "",
            ),
            (
                "Release identity",
                if rel.release_identity_ok {
                    "[OK]"
                } else {
                    "[WAIT]"
                },
                if rel.release_identity_ok {
                    "verified"
                } else {
                    "pending"
                },
                release_identity_detail.as_str(),
            ),
            (
                "Release pipeline",
                match attempt.release_pipeline_status.as_deref() {
                    Some("success") => "[OK]",
                    Some("running") => "[RUN]",
                    Some("failed") => "[FAIL]",
                    _ => "[WAIT]",
                },
                match attempt.release_pipeline_status.as_deref() {
                    Some(s) => s,
                    None => "not-started",
                },
                &rel.canary_state_path,
            ),
            (
                "Canary health",
                match rel.canary_state.as_str() {
                    "released" => "[OK]",
                    "in-flight" | "canary-authorized" => "[RUN]",
                    "failed" => "[FAIL]",
                    _ => "[WAIT]",
                },
                rel.canary_state.as_str(),
                match rel.canary_public_url.as_deref() {
                    Some(u) => u,
                    None => "",
                },
            ),
            (
                "E2E gate",
                match attempt.production_pipeline_status.as_deref() {
                    Some("success") => "[OK]",
                    Some("running") => "[RUN]",
                    Some("failed") => "[FAIL]",
                    _ => "[WAIT]",
                },
                "waiting on canary",
                &rel.gate_canary_e2e_path,
            ),
            (
                "Prod promotion",
                match attempt.production_pipeline_status.as_deref() {
                    Some("success") => "[OK]",
                    Some("running") => "[RUN]",
                    _ => "[WAIT]",
                },
                match attempt.production_pipeline_status.as_deref() {
                    Some(s) => s,
                    None => "not-triggered",
                },
                "",
            ),
        ];

        let header = Line::from(vec![Span::styled(
            format!(
                "  RELEASE: {}  Phase: {}  ",
                attempt.version, rel.canary_state
            ),
            Style::default()
                .fg(release_color(&rel.canary_state))
                .add_modifier(Modifier::BOLD),
        )]);
        let sep = Line::from(Span::styled(
            format!(
                "  {:-<width$}",
                "",
                width = gate_inner.width.saturating_sub(4) as usize
            ),
            Style::default().fg(Color::DarkGray),
        ));
        let col_header = Line::from(vec![Span::styled(
            format!("  {:<28} {:<7} {:<16} Detail", "Gate", "Status", "State"),
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]);

        let mut lines = vec![header, sep.clone(), col_header, sep.clone()];
        for (gate, badge, state, detail) in &gate_rows {
            let badge_color = match *badge {
                "[OK]" => Color::Green,
                "[RUN]" => Color::Blue,
                "[FAIL]" => Color::Red,
                _ => Color::Yellow,
            };
            let short_detail = short_text(detail, 20);
            lines.push(Line::from(vec![
                Span::raw(format!("  {:<28} ", gate)),
                Span::styled(
                    format!("{:<7} ", badge),
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<16} ", state), Style::default().fg(Color::White)),
                Span::styled(short_detail, Style::default().fg(Color::DarkGray)),
            ]));
        }
        f.render_widget(Paragraph::new(lines), gate_inner);
    } else {
        f.render_widget(
            Paragraph::new("\n  No release in progress.\n  Waiting for first green main pipeline.")
                .style(Style::default().fg(Color::DarkGray)),
            gate_inner,
        );
    }

    draw_release_inspector(f, app, cols[1]);
}
