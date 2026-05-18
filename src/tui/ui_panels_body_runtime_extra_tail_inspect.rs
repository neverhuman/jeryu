use super::super::*;

#[allow(dead_code)]
pub(crate) fn draw_pipeline_nav(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .state
        .pipelines
        .iter()
        .enumerate()
        .map(|(i, pm)| {
            let selected = i == app.selected_pipeline_index;
            let color = status_color(&pm.pipeline.status);
            let prefix = if selected { ">" } else { " " };
            let short_ref = short_text(&pm.pipeline.ref_name, 14);
            let line = Line::from(vec![
                Span::styled(
                    format!("{} #{:<6} ", prefix, pm.pipeline.pipeline_id),
                    Style::default().fg(if selected {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(short_ref, Style::default().fg(color)),
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
            .title(" Pipelines ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(list, area);
}

pub(crate) fn draw_job_inspector_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Inspector ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pane_border(ActivePane::Jobs, app)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(job) = app.selected_job() else {
        f.render_widget(
            Paragraph::new("\n  Choose a job with ↑↓").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    };

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("Job  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("#{}", job.job_id),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Name ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                job.job_name.as_deref().unwrap_or("?"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Status ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &job.status,
                Style::default()
                    .fg(status_color(&job.status))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Pool ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                job.pool_name.as_deref().unwrap_or("-"),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(Span::styled(
            "─────────────────",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    if let Some(ref cap) = app.state.inspector_capsule {
        lines.push(Line::from(Span::styled(
            "Evidence:",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(vec![
            Span::styled("  exit:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} ({})", cap.exit_code, cap.failure_kind),
                Style::default().fg(Color::Red),
            ),
        ]));
        // Show first 3 lines of log snippet
        for snippet_line in cap.log_snippet.lines().take(3) {
            lines.push(Line::from(Span::styled(
                format!("  {}", short_text(snippet_line, 28)),
                Style::default().fg(Color::Yellow),
            )));
        }
        lines.push(Line::from(Span::styled(
            "─────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Actions:",
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(Span::styled(
            "  [r] Retry job",
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            "  [d] Remove event",
            Style::default().fg(Color::White),
        )));
        lines.push(Line::from(Span::styled(
            "─────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Blocked:",
            Style::default().fg(Color::DarkGray),
        )));
        if job.status != "success" {
            lines.push(Line::from(Span::styled(
                "  Promote — not green",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else if job.status == "failed" {
        lines.push(Line::from(Span::styled(
            "  No capsule found",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  (evidence not stored yet)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  No evidence",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "  Actions:",
            Style::default().fg(Color::Cyan),
        )));
        lines.push(Line::from(Span::styled(
            "  [r] Retry  [d] Remove",
            Style::default().fg(Color::White),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

// ---------------------------------------------------------------------------
// Tab 4 — Agents: mission/session cockpit
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_more.rs"]
mod ui_panels_body_more;
pub(crate) use ui_panels_body_more::*;
