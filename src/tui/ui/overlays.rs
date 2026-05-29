//! Owner: Interactive TUI subsystem — global overlays (command palette + help)
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Pure draw. These are cross-cutting overlays the orchestrator
//!             draws over whatever lens is active. Relocated out of the deleted
//!             legacy `ui_panels.*` chain when the Flight Deck cutover gutted it;
//!             they depend only on `ui_chrome` helpers (via `super::*`), the app
//!             state, and the action registry — never on the old panels.

use super::*;

/// Truncate `input` to `max_chars` with an ellipsis (relocated from the deleted
/// legacy panels so the overlays stay self-contained).
fn short_text(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let text = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", text)
    } else {
        text
    }
}

// ===========================================================================
// Command palette
// ===========================================================================

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
        .title(" Command Palette — ↑↓ navigate  Enter execute ")
        .title_top(Line::from(" [esc] ").right_aligned())
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

// ===========================================================================
// Help overlay
// ===========================================================================

pub(crate) fn draw_help_overlay(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup_w = 60u16.min(area.width.saturating_sub(4));
    let popup_h = 22u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    let popup = Rect::new(x, y, popup_w, popup_h);

    f.render_widget(Clear, popup);

    let tab_name = match app.active_tab {
        ActiveTab::Workflow => "Workflow",
        ActiveTab::Mission => "Mission",
        ActiveTab::Release => "Release",
        ActiveTab::Approvals => "Approvals",
        ActiveTab::Jobs => "Jobs",
        ActiveTab::Agents => "Agents",
        ActiveTab::Tests => "Tests",
        ActiveTab::Pools => "Pools",
        ActiveTab::Cache => "Cache",
        ActiveTab::Evidence => "Evidence",
        ActiveTab::Repos => "Repos",
        ActiveTab::Bugs => "Bugs",
        ActiveTab::LLMs => "LLMs",
        ActiveTab::Git => "Git",
        ActiveTab::Secrets => "Secrets",
        ActiveTab::Jankurai => "Jankurai",
    };

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!(" Keyboard Shortcuts — {tab_name} Tab"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        help_row("1-0", "Switch to numbered tab"),
        help_row("Tab", "Cycle to next tab"),
        help_row("Ctrl-K", "Open command palette"),
        help_row("?", "Toggle this help overlay"),
        help_row("F5", "Force refresh all data"),
        help_row("q / Esc", "Quit TUI"),
        Line::from(""),
    ];

    // Tab-specific bindings
    match app.active_tab {
        ActiveTab::Workflow => {
            lines.push(Line::from(Span::styled(
                " ── Workflow ──",
                Style::default().fg(Color::Cyan),
            )));
            lines.push(help_row("↑↓←→", "Move between panes / drilled selection"));
            lines.push(help_row("Enter", "Open Inspector (on canvas)"));
            lines.push(help_row("Tab/BackTab", "Cycle Inspector sub-tabs"));
            lines.push(help_row("f", "Toggle follow active node"));
            lines.push(help_row("b", "Jump to next blocker"));
            lines.push(help_row("c", "Jump to critical-path head"));
            lines.push(help_row("z", "Cycle DAG zoom"));
            lines.push(help_row("r", "Trigger rollback (Promote node)"));
            lines.push(help_row("</>", "Cycle PR"));
        }
        ActiveTab::Jobs => {
            lines.push(Line::from(Span::styled(
                " ── Runner Feed ──",
                Style::default().fg(Color::Cyan),
            )));
            lines.push(help_row("f", "Freeze/unfreeze auto-cycle"));
            lines.push(help_row("n", "Next runner"));
            lines.push(help_row("N", "Previous runner"));
            lines.push(help_row("g", "Toggle follow-tail mode"));
            lines.push(help_row("Enter", "Open full-screen log view"));
            lines.push(help_row("c", "Cancel selected job"));
            lines.push(help_row("r", "Retry failed job"));
            lines.push(help_row("d", "Remove job record"));
        }
        ActiveTab::Tests => {
            lines.push(help_row("v / t", "Toggle average/latest view"));
            lines.push(help_row("Enter", "Show test history"));
            lines.push(help_row("↑↓", "Choose test"));
        }
        ActiveTab::Pools => {
            lines.push(help_row("p", "Pause/resume selected pool"));
        }
        ActiveTab::Evidence => {
            lines.push(help_row("a", "Toggle capsules/audit ledger"));
        }
        ActiveTab::LLMs => {
            lines.push(help_row("F5", "Refresh model policy and key sources"));
        }
        _ => {
            lines.push(help_row("↑↓", "Navigate items"));
            lines.push(help_row("Enter", "Inspect selected item"));
        }
    }

    let block = Block::default()
        .title(" [ Help ] ")
        .title_top(Line::from(" [esc] ").right_aligned())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        popup,
    );
}

pub(crate) fn help_row(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<12}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(desc.to_string(), Style::default().fg(Color::White)),
    ])
}
