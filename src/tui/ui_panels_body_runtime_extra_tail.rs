use super::*;

pub(crate) fn draw_pipeline_progress(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Pipeline Progress ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(ref progress) = app.state.pipeline_progress_view else {
        f.render_widget(Paragraph::new("  No active pipeline"), inner);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Pipeline header
    lines.push(Line::from(vec![
        Span::styled(
            format!("  #{} ", progress.pipeline_id),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}@{}", progress.ref_name, progress.sha_short),
            Style::default().fg(Color::White),
        ),
    ]));

    // Stage rows
    let tick = app.tick_count;
    for stage in &progress.stages {
        let (icon, icon_color) = match stage.status.as_str() {
            "success" => ("●", Color::Green),
            "running" => {
                // Animated indicator
                if tick % 4 < 2 {
                    ("◉", Color::Cyan)
                } else {
                    ("◎", Color::Cyan)
                }
            }
            "failed" => {
                if tick % 4 < 2 {
                    ("✕", Color::Red)
                } else {
                    ("✕", Color::LightRed)
                }
            }
            _ => ("○", Color::Yellow),
        };

        let bar_width = 16usize;
        let fill = if stage.total_jobs > 0 {
            (stage.completed_jobs * bar_width + stage.total_jobs / 2) / stage.total_jobs
        } else {
            0
        };
        let running_fill = if stage.total_jobs > 0 && stage.running_jobs > 0 {
            1.max((stage.running_jobs * bar_width) / stage.total_jobs)
        } else {
            0
        };
        let bar = format!(
            "{}{}{}",
            "█".repeat(fill),
            "▓".repeat(running_fill.min(bar_width - fill)),
            "░".repeat(bar_width.saturating_sub(fill + running_fill)),
        );

        let count_label = format!(
            "{}/{}",
            stage.completed_jobs + stage.running_jobs,
            stage.total_jobs
        );

        lines.push(Line::from(vec![
            Span::styled(format!("  {icon} "), Style::default().fg(icon_color)),
            Span::styled(
                format!("{:<12}", short_text(&stage.stage_name, 12)),
                Style::default().fg(Color::White),
            ),
            Span::styled(bar, Style::default().fg(icon_color)),
            Span::styled(
                format!(" {:<5}", count_label),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    // Overall progress bar
    lines.push(Line::from(""));
    let overall_bar = meter_bar(progress.overall_pct, 20);
    let eta_label = match progress.eta_remaining_secs {
        Some(secs) if secs >= 3600 => format!(
            "ETA ~{}h{}m ({})",
            secs / 3600,
            (secs % 3600) / 60,
            progress.eta_confidence
        ),
        Some(secs) if secs >= 60 => format!(
            "ETA ~{}m{}s ({})",
            secs / 60,
            secs % 60,
            progress.eta_confidence
        ),
        Some(secs) => format!("ETA ~{}s ({})", secs, progress.eta_confidence),
        None => "ETA unknown".into(),
    };

    lines.push(Line::from(vec![
        Span::styled(format!("  {overall_bar}"), Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("  {eta_label}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// TUI v2 — Job Matrix
// ---------------------------------------------------------------------------

pub(crate) fn draw_job_matrix(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Job Matrix ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Group jobs by stage/pool
    let mut groups: Vec<(&str, Vec<&crate::state::JobEvent>)> = Vec::new();
    let mut current_stage: Option<&str> = None;

    for job in &app.state.recent_jobs {
        let stage = job.pool_name.as_deref().unwrap_or("default");
        if current_stage != Some(stage) {
            groups.push((stage, vec![job]));
            current_stage = Some(stage);
        } else if let Some(last) = groups.last_mut() {
            last.1.push(job);
        }
    }

    let tick = app.tick_count;
    let mut lines: Vec<Line> = Vec::new();
    for (stage_name, jobs) in groups.iter().take(inner.height as usize) {
        let mut spans: Vec<Span> = vec![Span::styled(
            format!("  {:<14}", short_text(stage_name, 14)),
            Style::default().fg(Color::DarkGray),
        )];
        for job in jobs.iter().take(20) {
            let (dot, color) = match job.status.as_str() {
                "success" => ("●", Color::Green),
                "running" => {
                    if tick % 4 < 2 {
                        ("●", Color::Cyan)
                    } else {
                        ("◌", Color::Cyan)
                    }
                }
                "failed" => {
                    if tick % 6 < 3 {
                        ("●", Color::Red)
                    } else {
                        ("●", Color::LightRed)
                    }
                }
                "pending" | "created" => ("○", Color::Yellow),
                "canceled" => ("○", Color::DarkGray),
                _ => ("○", Color::Gray),
            };
            spans.push(Span::styled(format!("{dot} "), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No jobs tracked",
            Style::default().fg(Color::DarkGray),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ---------------------------------------------------------------------------
// Pipeline nav + Job inspector + Agents tab (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_runtime_extra_tail_inspect.rs"]
mod ui_panels_body_runtime_extra_tail_inspect;
pub(crate) use ui_panels_body_runtime_extra_tail_inspect::*;
