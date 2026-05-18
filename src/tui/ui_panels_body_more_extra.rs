use super::*;

#[path = "ui_panels_body_more_cache.rs"]
mod ui_panels_body_more_cache;
pub(crate) use ui_panels_body_more_cache::draw_cache_dashboard;

pub(crate) fn agent_phase_for_status(status: &str) -> &'static str {
    match status {
        "success" => "review",
        "failed" => "blocked",
        "running" => "testing",
        "pending" | "created" => "queued",
        "canceled" => "stopped",
        _ => "working",
    }
}

pub(crate) fn draw_agent_actions(f: &mut Frame, app: &App, area: Rect) {
    let selected_status = app
        .state
        .agent_pipelines
        .get(app.selected_job_index)
        .map(|p| p.status.as_str())
        .unwrap_or("idle");
    let lines = vec![
        Line::from(Span::styled(
            "  Authority",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  write branch ", Style::default().fg(Color::DarkGray)),
            Span::styled("grant required", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("  merge        ", Style::default().fg(Color::DarkGray)),
            Span::styled("proof required", Style::default().fg(Color::Magenta)),
        ]),
        Line::from(vec![
            Span::styled("  sandbox      ", Style::default().fg(Color::DarkGray)),
            Span::styled("strict fail-closed", Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Available actions",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  ^K explain blockers",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  ^K fetch capsule",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            if selected_status == "success" {
                "  ^K request merge proof"
            } else {
                "  ^K run validation"
            },
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            "  ^K revoke grant",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" [ Actions / Grants ] ")
                    .borders(Borders::ALL)
                    .border_style(focus::border_style(app, PaneId::AgentsActions)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

// ---------------------------------------------------------------------------
// Tab 8 — Evidence: failure capsule viewer
// ---------------------------------------------------------------------------

pub(crate) fn draw_evidence_tab(f: &mut Frame, app: &mut App, area: Rect) {
    use crate::tui::app::EvidenceViewMode;
    if app.evidence_view_mode == EvidenceViewMode::AuditLedger {
        draw_audit_ledger(f, app, area);
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    focus::register_pane(app, PaneId::EvidenceList, cols[0]);
    focus::register_pane(app, PaneId::EvidenceDetail, cols[1]);

    // Left: evidence record list
    let items: Vec<ListItem> = app
        .state
        .recent_evidence
        .iter()
        .enumerate()
        .map(|(i, rec)| {
            let selected = i == app.selected_evidence_index;
            let prefix = if selected { "> " } else { "  " };
            let ts = rec.created_at.get(..16).unwrap_or(&rec.created_at);
            let kind_color = match rec.failure_kind.as_str() {
                "compile_failure" => Color::Red,
                "test_failure" => Color::LightRed,
                "timeout" => Color::Yellow,
                "network" => Color::Cyan,
                "quarantined" => Color::Magenta,
                _ => Color::Gray,
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{}{} ", prefix, ts),
                    Style::default().fg(if selected {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(
                    format!("job#{:<6} ", rec.job_id),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    short_text(&rec.failure_kind, 14),
                    Style::default().fg(kind_color),
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

    let list = List::new(items).block(
        Block::default()
            .title(format!(
                " [ Evidence Capsules ({}) — 'a': audit ledger ] ",
                app.state.recent_evidence.len()
            ))
            .borders(Borders::ALL)
            .border_style(focus::border_style(app, PaneId::EvidenceList)),
    );
    f.render_widget(list, cols[0]);

    // Right: capsule detail
    let detail_block = Block::default()
        .title(" [ Capsule Detail ] ")
        .borders(Borders::ALL)
        .border_style(focus::border_style(app, PaneId::EvidenceDetail));
    let detail_inner = detail_block.inner(cols[1]);
    f.render_widget(detail_block, cols[1]);

    if let Some(rec) = app.state.recent_evidence.get(app.selected_evidence_index) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("job:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("#{}", rec.job_id), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("ref:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(&rec.ref_name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("sha:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    rec.commit_sha.get(..12).unwrap_or(&rec.commit_sha),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::styled("stage:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&rec.stage, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("exit:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", rec.exit_code),
                    Style::default().fg(Color::Red),
                ),
            ]),
            Line::from(vec![
                Span::styled("kind:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(&rec.failure_kind, Style::default().fg(Color::LightRed)),
            ]),
            Line::from(Span::styled(
                "─────────────────",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        // Show causal link if available
        if let Ok(cap) = serde_json::from_str::<crate::capsule::FailureCapsule>(&rec.payload) {
            if let Some(sup) = &cap.superseded_by_sha {
                let sup_short = sup.get(..12).unwrap_or(sup).to_string();
                lines.push(Line::from(vec![
                    Span::styled("superseded: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(sup_short, Style::default().fg(Color::Yellow)),
                ]));
            }
            if let Some(requeue_id) = cap.requeued_from_job_id {
                lines.push(Line::from(vec![
                    Span::styled("requeue_of: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("job#{}", requeue_id),
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
            }
            lines.push(Line::from(Span::styled(
                "Log snippet:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            let snippet_width = (detail_inner.width as usize).saturating_sub(4);
            for snippet_line in cap.log_snippet.lines().take(6) {
                lines.push(Line::from(Span::styled(
                    format!("  {}", short_text(snippet_line, snippet_width)),
                    Style::default().fg(Color::White),
                )));
            }
        }

        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            detail_inner,
        );
    } else {
        f.render_widget(
            Paragraph::new("\n  No evidence records.\n  Capsules appear here when jobs fail.")
                .style(Style::default().fg(Color::DarkGray)),
            detail_inner,
        );
    }
}
