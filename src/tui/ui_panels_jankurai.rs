use super::*;

#[path = "ui_panels_jankurai_helpers.rs"]
mod ui_panels_jankurai_helpers;

#[path = "ui_panels_jankurai_panels.rs"]
mod ui_panels_jankurai_panels;

pub(crate) fn draw_jank_tab(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Min(12),
        ])
        .split(area);
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(rows[0]);
    let middle_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(rows[1]);
    let bottom_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(rows[2]);

    let scan = app.state.jankurai.last_scan.as_ref();
    let history = &app.state.jankurai.history;

    // ── Summary block (top-left) ────────────────────────────────────────
    render_summary_block(f, top_cols[0], scan, history);

    // ── Status block (top-right) ────────────────────────────────────────
    render_status_block(f, top_cols[1], app);

    // ── Score history chart (middle-left) ───────────────────────────────
    render_score_chart(f, middle_cols[0], history);

    // ── Dimension breakdown (middle-right) ──────────────────────────────
    let breakdown_block = Block::default()
        .title(" [ Last Scan Dimensions ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let breakdown_inner = breakdown_block.inner(middle_cols[1]);
    f.render_widget(breakdown_block, middle_cols[1]);
    ui_panels_jankurai_panels::render_breakdown_panel(
        f,
        middle_cols[1],
        &app.state.jankurai.dimensions,
        breakdown_inner,
    );

    // ── Caps / Findings list (bottom-left) ──────────────────────────────
    let issues_block = Block::default()
        .title(" [ Caps / Findings ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let issues_inner = issues_block.inner(bottom_cols[0]);
    f.render_widget(issues_block, bottom_cols[0]);
    ui_panels_jankurai_panels::render_issues_panel(
        f,
        bottom_cols[0],
        &app.state.jankurai.entries,
        app.selected_jankurai_index,
        issues_inner,
    );

    // ── Entry detail (bottom-right) ─────────────────────────────────────
    let detail_block = Block::default()
        .title(" [ Entry Detail ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let detail_inner = detail_block.inner(bottom_cols[1]);
    f.render_widget(detail_block, bottom_cols[1]);
    if let Some(entry) = app.selected_jankurai_entry() {
        ui_panels_jankurai_panels::render_entry_detail(f, bottom_cols[1], entry, detail_inner);
    } else {
        f.render_widget(
            Paragraph::new("  No Jankurai entry selected.")
                .style(Style::default().fg(Color::DarkGray)),
            detail_inner,
        );
    }
}

fn render_summary_block(
    f: &mut Frame,
    area: Rect,
    scan: Option<&crate::tui::jankurai::JankuraiScan>,
    history: &[crate::tui::jankurai::JankuraiHistoryPoint],
) {
    use ui_panels_jankurai_helpers::scan_text;

    let score_text = scan_text(scan, |s| s.score.to_string(), "n/a");
    let raw_score_text = scan_text(scan, |s| s.raw_score.to_string(), "n/a");
    let minimum_score_text = scan_text(scan, |s| s.minimum_score.to_string(), "n/a");
    let decision_text = scan_text(scan, |s| s.decision.clone(), "n/a");
    let score_status_text = scan_text(scan, |s| s.score_status.clone(), "n/a");
    let generated_at_text = match scan {
        Some(s) => match &s.generated_at {
            Some(ts) => ui_panels_jankurai_helpers::format_timestamp(ts),
            None => "n/a".into(),
        },
        None => "n/a".into(),
    };
    let finding_count_text = scan_text(scan, |s| s.finding_count.to_string(), "0");
    let hard_findings_text = scan_text(scan, |s| s.hard_findings.to_string(), "0");
    let soft_findings_text = scan_text(scan, |s| s.soft_findings.to_string(), "0");
    let cap_count_text = scan_text(scan, |s| s.caps_applied.len().to_string(), "0");

    let summary_lines = vec![
        Line::from(vec![
            Span::styled("score:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                score_text,
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   raw: ", Style::default().fg(Color::DarkGray)),
            Span::styled(raw_score_text, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("min:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(minimum_score_text, Style::default().fg(Color::Yellow)),
            Span::styled("   decision: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                decision_text,
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("status:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(score_status_text, Style::default().fg(Color::Cyan)),
            Span::styled("   at: ", Style::default().fg(Color::DarkGray)),
            Span::styled(generated_at_text, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("findings:", Style::default().fg(Color::DarkGray)),
            Span::styled(finding_count_text, Style::default().fg(Color::Red)),
            Span::styled(" hard:", Style::default().fg(Color::DarkGray)),
            Span::styled(hard_findings_text, Style::default().fg(Color::Red)),
            Span::styled(" soft", Style::default().fg(Color::DarkGray)),
            Span::styled(soft_findings_text, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("caps:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(cap_count_text, Style::default().fg(Color::Magenta)),
            Span::styled("   history points: ", Style::default().fg(Color::DarkGray)),
            Span::styled(history.len().to_string(), Style::default().fg(Color::White)),
        ]),
    ];

    let summary_block = Block::default()
        .title(" [ Jankurai Summary ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let summary_inner = summary_block.inner(area);
    f.render_widget(summary_block, area);
    f.render_widget(Paragraph::new(summary_lines), summary_inner);
}

fn render_status_block(f: &mut Frame, area: Rect, app: &App) {
    let status_block = Block::default()
        .title(" [ Jankurai Status ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.state.jankurai.error.is_some() {
            Color::Red
        } else {
            Color::DarkGray
        }));
    let status_inner = status_block.inner(area);
    f.render_widget(status_block, area);

    if let Some(error) = &app.state.jankurai.error {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Parse / load error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    short_text(error, status_inner.width.saturating_sub(2) as usize),
                    Style::default().fg(Color::White),
                )),
            ])
            .wrap(Wrap { trim: false }),
            status_inner,
        );
    } else {
        let installed = if app.jankurai_available() { "installed" } else { "not installed" };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Jankurai",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("PATH: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(installed, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("points: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        app.state.jankurai.history.len().to_string(),
                        Style::default().fg(Color::Green),
                    ),
                ]),
            ])
            .wrap(Wrap { trim: false }),
            status_inner,
        );
    }
}

fn render_score_chart(
    f: &mut Frame,
    area: Rect,
    history: &[crate::tui::jankurai::JankuraiHistoryPoint],
) {
    use ui_panels_jankurai_helpers::{chart_labels, y_axis_labels};

    let chart_block = Block::default()
        .title(" [ Score History ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let chart_inner = chart_block.inner(area);
    f.render_widget(chart_block, area);

    if history.is_empty() {
        f.render_widget(
            Paragraph::new("  No Jankurai history found")
                .style(Style::default().fg(Color::DarkGray)),
            chart_inner,
        );
        return;
    }

    if chart_inner.width < 40 || chart_inner.height < 6 {
        let scores: Vec<i64> = history.iter().map(|p| p.score).collect();
        let spark = crate::tui::widgets::sparkline::spark_i64(
            &scores,
            chart_inner.width.saturating_sub(4) as usize,
            Color::Cyan,
        );
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("score: ", Style::default().fg(Color::DarkGray)),
                    spark,
                ]),
                Line::from(vec![
                    Span::styled("range: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(
                            "{} -> {}",
                            scores.iter().min().copied().unwrap_or(0),
                            scores.iter().max().copied().unwrap_or(0)
                        ),
                        Style::default().fg(Color::White),
                    ),
                ]),
            ]),
            chart_inner,
        );
        return;
    }

    let data: Vec<(f64, f64)> = history
        .iter()
        .enumerate()
        .map(|(i, p)| (i as f64, p.score as f64))
        .collect();
    let labels = chart_labels(history);
    // Zoom Y-axis to the actual score range so the trend line is clearly visible
    // instead of appearing as a flat line at the top of a 0-100 scale.
    let y_min = data.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
    let y_max = data.iter().map(|(_, y)| *y).fold(f64::NEG_INFINITY, f64::max);
    let y_pad = ((y_max - y_min) * 0.3).max(5.0);
    let y_lo = (y_min - y_pad).max(0.0);
    let y_hi = (y_max + y_pad).min(100.0);
    let chart = Chart::new(vec![
        Dataset::default()
            .name("score")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&data),
    ])
    .block(Block::default())
    .x_axis(
        Axis::default()
            .title("time")
            .style(Style::default().fg(Color::DarkGray))
            .bounds([0.0, (data.len().saturating_sub(1)).max(1) as f64])
            .labels(labels.0),
    )
    .y_axis(
        Axis::default()
            .title("score")
            .style(Style::default().fg(Color::DarkGray))
            .bounds([y_lo, y_hi])
            .labels(y_axis_labels(y_lo, y_hi)),
    );
    f.render_widget(chart, chart_inner);
}
