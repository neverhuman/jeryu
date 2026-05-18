use super::*;

pub(crate) fn feed_line_color(line: &str) -> Color {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("fatal")
        || lower.contains("panicked")
    {
        Color::Red
    } else if lower.contains("warning") || lower.contains("warn") {
        Color::Yellow
    } else if lower.contains("compiling")
        || lower.contains("running")
        || lower.contains("downloading")
        || lower.contains("fetching")
    {
        Color::Cyan
    } else if lower.contains("finished")
        || lower.contains("test result: ok")
        || lower.contains("passed")
        || lower.contains("... ok")
    {
        Color::Green
    } else if line.starts_with('[') && line.len() > 10 {
        // Timestamp prefix — dim it
        Color::DarkGray
    } else {
        Color::White
    }
}

pub(crate) fn format_elapsed(secs: f64) -> String {
    let total = secs as u64;
    if total >= 3600 {
        format!("{}h{}m{}s", total / 3600, (total % 3600) / 60, total % 60)
    } else if total >= 60 {
        format!("{}m{}s", total / 60, total % 60)
    } else {
        format!("{}s", total)
    }
}

pub(crate) fn draw_live_runner_feed(f: &mut Frame, app: &App, area: Rect) {
    let feeds = &app.state.runner_feeds;
    let active_idx = app.state.active_feed_index;
    let is_cycling = app.feed_pinned.is_none();

    let cycle_label = if is_cycling {
        "⟳ cycling 5s"
    } else {
        "⏸ pinned"
    };
    let runner_label = if feeds.is_empty() {
        "no runners".to_string()
    } else {
        format!("runner {}/{}", active_idx + 1, feeds.len())
    };

    let block = Block::default()
        .title(format!(
            " Live Runner Feed ── {} ── {} ",
            cycle_label, runner_label
        ))
        .borders(Borders::ALL)
        .border_style(focus::border_style(app, PaneId::JobsRunnerFeed));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if feeds.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No active runners. Waiting for CI jobs...",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Tip: Start a pipeline to see live logs here.",
                    Style::default().fg(Color::DarkGray),
                )),
            ]),
            inner,
        );
        return;
    }

    let feed = &feeds[active_idx.min(feeds.len().saturating_sub(1))];

    // Split into header (2 lines) + logs area + indicator strip (1 line)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Runner header
            Constraint::Min(4),    // Log content
            Constraint::Length(1), // Runner indicator strip
        ])
        .split(inner);

    // Runner header
    let feed_color = status_color(&feed.status);
    let header_spans = vec![
        Span::styled(
            format!(" {} ", &feed.runner_name),
            Style::default()
                .fg(Color::Black)
                .bg(feed_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" │ {} ", &feed.job_name),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("│ ⏱ {} ", format_elapsed(feed.elapsed_secs)),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            format!("│ job #{}", feed.job_id),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    f.render_widget(Paragraph::new(Line::from(header_spans)), rows[0]);

    // Log content with color coding
    let log_lines: Vec<Line> = feed
        .log_tail
        .lines()
        .map(|line| {
            let color = feed_line_color(line);
            let style = if color == Color::Red || color == Color::Green {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect();

    let total_lines = log_lines.len() as u16;
    let visible_height = rows[1].height;
    let scroll_offset = if app.feed_follow_tail {
        total_lines.saturating_sub(visible_height)
    } else {
        app.feed_scroll_offset
            .min(total_lines.saturating_sub(visible_height))
    };

    f.render_widget(
        Paragraph::new(log_lines).scroll((scroll_offset, 0)),
        rows[1],
    );

    // Runner indicator strip
    let mut indicator_spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, f_entry) in feeds.iter().enumerate() {
        let is_active = i == active_idx;
        let dot_color = status_color(&f_entry.status);
        let dot = if f_entry.status == "running" || f_entry.status == "pending" {
            "●"
        } else if f_entry.status == "failed" {
            "✕"
        } else {
            "○"
        };
        let name = short_text(&f_entry.runner_name, 12);
        if is_active {
            indicator_spans.push(Span::styled(
                format!("{dot} {name} "),
                Style::default()
                    .fg(dot_color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        } else {
            indicator_spans.push(Span::styled(
                format!("{dot} {name} "),
                Style::default().fg(dot_color),
            ));
        }
        indicator_spans.push(Span::styled(" ", Style::default()));
    }
    f.render_widget(Paragraph::new(Line::from(indicator_spans)), rows[2]);
}

// ---------------------------------------------------------------------------
// TUI v2 — Pipeline Progress
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_runtime_extra_tail.rs"]
mod ui_panels_body_runtime_extra_tail;
pub(crate) use ui_panels_body_runtime_extra_tail::*;
