use super::*;

pub(crate) fn draw_command_palette(f: &mut Frame, app: &App) {
    use crate::tui::action_registry;

    let screen = f.area();
    let modal_w = (screen.width as f32 * 0.60) as u16;
    let modal_h = (screen.height as f32 * 0.60) as u16;
    let modal_x = (screen.width.saturating_sub(modal_w)) / 2;
    let modal_y = (screen.height.saturating_sub(modal_h)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_w, modal_h);

    // Clear the area
    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" Command Palette — type to filter, ↑↓ navigate, Enter execute, Esc close ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    // Split: input line at top, action list + preview below
    let splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(2)])
        .split(inner);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(splits[1]);

    // Input row
    let input_line = Line::from(vec![
        Span::styled(
            "> ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}_", app.command_palette_query),
            Style::default().fg(Color::White),
        ),
    ]);
    let input_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));
    let input_inner = input_block.inner(splits[0]);
    f.render_widget(input_block, splits[0]);
    f.render_widget(Paragraph::new(input_line), input_inner);

    // Filtered action list
    let matches: Vec<&action_registry::ActionEntry> =
        action_registry::filtered(&app.command_palette_query).collect();

    if matches.is_empty() {
        f.render_widget(
            Paragraph::new("  No matching actions.").style(Style::default().fg(Color::DarkGray)),
            body[0],
        );
        return;
    }

    let items: Vec<ListItem> = matches
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let selected = i == app.selected_palette_index;
            let bg = if selected {
                Color::DarkGray
            } else {
                Color::Reset
            };
            let risk_color = entry.risk_tier.color();
            let key_hint = match entry.key_hint {
                Some(k) => format!(" [{k}]"),
                None => String::new(),
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {:<28}", entry.label),
                    Style::default().fg(Color::White).bg(bg),
                ),
                Span::styled(
                    format!("{:<6}", entry.risk_tier.label()),
                    Style::default().fg(risk_color).bg(bg),
                ),
                Span::styled(
                    format!("{:<6}", key_hint),
                    Style::default().fg(Color::DarkGray).bg(bg),
                ),
                Span::styled(
                    format!(
                        "  {}",
                        short_text(
                            entry.description,
                            (body[0].width as usize).saturating_sub(46)
                        )
                    ),
                    Style::default().fg(Color::DarkGray).bg(bg),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default())
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_widget(list, body[0]);

    // Column header
    let header = Paragraph::new(Line::from(vec![Span::styled(
        format!(" {:<28}{:<6}{:<6}  Description", "Action", "Risk", "Key"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )]));
    // Render header over the top of the action list.
    if body[0].height > 2 {
        let header_area = Rect::new(body[0].x, body[0].y, body[0].width, 1);
        f.render_widget(header, header_area);
    }

    let selected = matches
        .get(app.selected_palette_index)
        .copied()
        .unwrap_or(matches[0]);
    draw_action_preview(f, app, selected, body[1]);
}

pub(crate) fn draw_action_preview(
    f: &mut Frame,
    app: &App,
    entry: &crate::tui::action_registry::ActionEntry,
    area: Rect,
) {
    let enabled_reason = action_enabled_reason(app, entry.id);
    let enabled = enabled_reason.is_none();
    let risk = entry.risk_tier.label();
    let risk_color = entry.risk_tier.color();
    let side_effect = entry.side_effect_class().label();
    let grant = entry.required_grant().label();
    let lines = vec![
        Line::from(Span::styled(
            entry.label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Risk:        ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                risk,
                Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Side effect: ", Style::default().fg(Color::DarkGray)),
            Span::styled(side_effect, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Grant:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                grant,
                Style::default().fg(if grant == "none" {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Dry run:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if entry.dry_run {
                    "available"
                } else {
                    "not declared"
                },
                Style::default().fg(if entry.dry_run {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Status:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if enabled { "enabled" } else { "disabled" },
                Style::default()
                    .fg(if enabled { Color::Green } else { Color::Red })
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "What will happen",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            short_text(entry.description, area.width.saturating_sub(4) as usize),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            match enabled_reason {
                Some(value) => value,
                None => {
                    "Ready. Press Enter to execute or preview via the matching CLI/API surface."
                        .to_string()
                }
            },
            Style::default().fg(if enabled { Color::Green } else { Color::Yellow }),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" [ Preview / Blast Radius ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(risk_color)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub(crate) fn action_enabled_reason(app: &App, action_id: &str) -> Option<String> {
    match action_id {
        "requeue_job" => {
            let Some(job) = app.selected_job() else {
                return Some("Choose a failed or canceled job first.".to_string());
            };
            if matches!(job.status.as_str(), "failed" | "canceled") {
                None
            } else {
                Some(format!("Current job status is '{}', not failed/canceled.", job.status))
            }
        }
        "remove_record" | "open_logs" => {
            if app.selected_job().is_some() {
                None
            } else {
                Some("Choose a job first.".to_string())
            }
        }
        "pause_pool" => {
            if app.state.pools.get(app.selected_pool_index).is_some() {
                None
            } else {
                Some("Choose a runner pool first.".to_string())
            }
        }
        "request_merge" => Some("Merge proof must be requested through the evidence-bound API; green UI state is intentionally not inferred.".to_string()),
        "propose_patch" | "race_patches" | "run_tests" => Some(
            "Requires a scoped capability grant and request envelope before side effects."
                .to_string(),
        ),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// TUI v2 — Help overlay (extracted to companion file)
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_tail_extra_tail_help.rs"]
mod ui_panels_body_tail_extra_tail_help;
pub(crate) use ui_panels_body_tail_extra_tail_help::*;
