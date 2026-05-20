use super::*;

pub(crate) fn draw_bugs_tab(f: &mut Frame, app: &mut App, area: Rect) {
    app.clamp_bug_selection();
    let vm = crate::tui::bugs::build_view_model(
        &app.state.bugs,
        app.bug_sort_mode,
        app.selected_bug_index,
        app.selected_bug_project_index,
    );
    app.selected_bug_index = vm.selected_bug_index;
    app.selected_bug_project_index = vm.selected_project_index;

    if area.width < 110 {
        draw_bugs_compact(f, app, area, &vm);
    } else {
        draw_bugs_wide(f, app, area, &vm);
    }
}

fn draw_bugs_wide(f: &mut Frame, app: &mut App, area: Rect, vm: &crate::tui::bugs::BugsViewModel) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30),
            Constraint::Percentage(52),
            Constraint::Percentage(48),
        ])
        .split(area);

    draw_projects(f, app, columns[0], vm);
    draw_table(f, app, columns[1], vm);
    draw_inspector(f, app, columns[2], vm);
}

fn draw_bugs_compact(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    vm: &crate::tui::bugs::BugsViewModel,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(12),
            Constraint::Min(8),
        ])
        .split(area);

    draw_projects(f, app, rows[0], vm);
    draw_table(f, app, rows[1], vm);
    draw_inspector(f, app, rows[2], vm);
}

fn draw_projects(f: &mut Frame, app: &mut App, area: Rect, vm: &crate::tui::bugs::BugsViewModel) {
    let pane = PaneId::BugsProjects;
    focus::register_pane(app, pane, area);
    focus::register_esc_hotspot(app, pane, area);
    let lines = if vm.projects.is_empty() {
        vec![Line::from(Span::styled(
            "  No registered bugs",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        vm.projects
            .iter()
            .map(|project| {
                Line::from(vec![
                    Span::styled(
                        if project.selected { "> " } else { "  " },
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        fit_text(&project.name, 16),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(
                        format!(
                            " open:{} ready:{} blocked:{}",
                            project.open, project.ready, project.blocked
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect()
    };

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(
                        " [ Bug Projects{} ] ",
                        focus::esc_label(focus::is_active(app, pane))
                    ))
                    .borders(Borders::ALL)
                    .border_style(focus::border_style(app, pane)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_table(f: &mut Frame, app: &mut App, area: Rect, vm: &crate::tui::bugs::BugsViewModel) {
    let pane = PaneId::BugsTable;
    focus::register_pane(app, pane, area);
    focus::register_esc_hotspot(app, pane, area);
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("  ID            ", Style::default().fg(Color::DarkGray)),
        Span::styled("S/P D ", Style::default().fg(Color::DarkGray)),
        Span::styled("Status        ", Style::default().fg(Color::DarkGray)),
        Span::styled("Attempts ", Style::default().fg(Color::DarkGray)),
        Span::styled("Route        ", Style::default().fg(Color::DarkGray)),
        Span::styled("Title", Style::default().fg(Color::DarkGray)),
    ]));

    if vm.rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Submit bugs with `jeryu bug submit --file report.json`.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.extend(vm.rows.iter().take(24).map(|bug| {
            let route = format!("{} -> {}", bug.source_project, bug.project);
            Line::from(vec![
                Span::styled(
                    if bug.selected { "> " } else { "  " },
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:<14}", fit_text(&bug.id, 14)),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{}/{} d{} ", bug.severity, bug.priority, bug.difficulty),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:<14}", bug.status.as_str()),
                    Style::default().fg(status_color(bug.status.as_str())),
                ),
                Span::styled(
                    format!("{}/{}      ", bug.attempt_count, bug.failed_attempt_count),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    format!("{:<13}", fit_text(&route, 13)),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(fit_text(&bug.title, 54), Style::default().fg(Color::White)),
            ])
        }));
    }

    let title = format!(
        " [ Bugs sort:{}{} ] ",
        vm.sort_mode.label(),
        focus::esc_label(focus::is_active(app, pane))
    );
    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(focus::border_style(app, pane)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_inspector(f: &mut Frame, app: &mut App, area: Rect, vm: &crate::tui::bugs::BugsViewModel) {
    let pane = PaneId::BugsInspector;
    focus::register_pane(app, pane, area);
    focus::register_esc_hotspot(app, pane, area);
    let lines = if let Some(bug) = &vm.selected {
        let mut lines = vec![
            Line::from(Span::styled(
                short_text(&bug.title, 72),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled("id: ", Style::default().fg(Color::DarkGray)),
                Span::styled(bug.id.clone(), Style::default().fg(Color::Cyan)),
                Span::styled("  status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    bug.status.as_str(),
                    Style::default().fg(status_color(bug.status.as_str())),
                ),
            ]),
            Line::from(vec![
                Span::styled("route: ", Style::default().fg(Color::DarkGray)),
                Span::styled(bug.route.clone(), Style::default().fg(Color::Gray)),
            ]),
            Line::from(vec![
                Span::styled("rank: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "{}/{} d{} attempts:{}",
                        bug.severity, bug.priority, bug.difficulty, bug.attempts
                    ),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::styled("component: ", Style::default().fg(Color::DarkGray)),
                Span::styled(bug.component.clone(), Style::default().fg(Color::Gray)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Current behavior",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(short_text(&bug.current_behavior, 92)),
            Line::from(Span::styled(
                "Expected behavior",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(short_text(&bug.expected_behavior, 92)),
            Line::from(Span::styled("Impact", Style::default().fg(Color::Yellow))),
            Line::from(short_text(&bug.impact, 92)),
            Line::from(Span::styled(
                "Reproduction",
                Style::default().fg(Color::Yellow),
            )),
        ];
        append_list(&mut lines, &bug.reproduction_steps, 4);
        lines.push(Line::from(Span::styled(
            "Evidence",
            Style::default().fg(Color::Yellow),
        )));
        append_list(&mut lines, &bug.evidence, 3);
        lines.push(Line::from(Span::styled(
            "Acceptance",
            Style::default().fg(Color::Yellow),
        )));
        append_list(&mut lines, &bug.acceptance_criteria, 3);
        lines
    } else {
        vec![Line::from(Span::styled(
            "No bug selected.",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(format!(
                        " [ Inspector{} ] ",
                        focus::esc_label(focus::is_active(app, pane))
                    ))
                    .borders(Borders::ALL)
                    .border_style(focus::border_style(app, pane)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn append_list(lines: &mut Vec<Line<'static>>, items: &[String], limit: usize) {
    if items.is_empty() {
        lines.push(Line::from("  missing"));
        return;
    }
    for item in items.iter().take(limit) {
        lines.push(Line::from(format!("  - {}", short_text(item, 88))));
    }
}

fn fit_text(input: &str, width: usize) -> String {
    let char_count = input.chars().count();
    if char_count <= width {
        return input.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let truncated = input
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}
