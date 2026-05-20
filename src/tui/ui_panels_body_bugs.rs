use super::*;

pub(crate) fn draw_bugs_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Percentage(52),
            Constraint::Percentage(48),
        ])
        .split(area);

    focus::register_pane(app, PaneId::BugsProjects, columns[0]);
    focus::register_pane(app, PaneId::BugsTable, columns[1]);
    focus::register_pane(app, PaneId::BugsInspector, columns[2]);

    let mut projects = std::collections::BTreeMap::<String, (usize, usize, usize)>::new();
    for bug in &app.state.bugs {
        let entry = projects.entry(bug.target_project.clone()).or_default();
        if !bug.status.is_terminal() {
            entry.0 += 1;
        }
        if bug.status == crate::bugtracker::BugStatus::Ready {
            entry.1 += 1;
        }
        if matches!(
            bug.status,
            crate::bugtracker::BugStatus::Blocked | crate::bugtracker::BugStatus::NeedsInfo
        ) {
            entry.2 += 1;
        }
    }

    let project_lines = if projects.is_empty() {
        vec![Line::from(Span::styled(
            "  No registered bugs",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        projects
            .into_iter()
            .map(|(project, (open, ready, blocked))| {
                Line::from(vec![
                    Span::styled(format!("  {project}"), Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!(" open:{open} ready:{ready} blocked:{blocked}"),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect()
    };
    f.render_widget(
        Paragraph::new(project_lines)
            .block(
                Block::default()
                    .title(" [ Bug Projects ] ")
                    .borders(Borders::ALL)
                    .border_style(focus::border_style(app, PaneId::BugsProjects)),
            )
            .wrap(Wrap { trim: false }),
        columns[0],
    );

    let rows = app
        .state
        .bugs
        .iter()
        .take(30)
        .map(|bug| {
            Line::from(vec![
                Span::styled(format!("{:<14}", bug.id), Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!(
                        "{:<3} {:<3} d{} ",
                        bug.severity.label(),
                        bug.priority.label(),
                        bug.difficulty
                    ),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:<13}", bug.status.as_str()),
                    Style::default().fg(status_color(bug.status.as_str())),
                ),
                Span::styled(
                    format!(" {:<12}", bug.target_project),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(
                    short_text(&bug.title, 58),
                    Style::default().fg(Color::White),
                ),
            ])
        })
        .collect::<Vec<_>>();
    let table_lines = if rows.is_empty() {
        vec![Line::from(Span::styled(
            "  Submit bugs with `jeryu bug submit --file report.json`.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        rows
    };
    f.render_widget(
        Paragraph::new(table_lines)
            .block(
                Block::default()
                    .title(" [ Bugs ] ")
                    .borders(Borders::ALL)
                    .border_style(focus::border_style(app, PaneId::BugsTable)),
            )
            .wrap(Wrap { trim: false }),
        columns[1],
    );

    let selected = app.state.bugs.get(
        app.selected_job_index
            .min(app.state.bugs.len().saturating_sub(1)),
    );
    let inspector_lines = if let Some(bug) = selected {
        let mut lines = vec![
            Line::from(Span::styled(
                short_text(&bug.title, 72),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::styled("id: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&bug.id, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("route: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} -> {}", bug.source_project, bug.target_project),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::styled("component: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    bug.component.as_deref().unwrap_or("-"),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled("Impact", Style::default().fg(Color::Yellow))),
            Line::from(short_text(&bug.impact, 92)),
            Line::from(""),
            Line::from(Span::styled(
                "Reproduction",
                Style::default().fg(Color::Yellow),
            )),
        ];
        if bug.body.reproduction_steps.is_empty() {
            lines.push(Line::from("missing"));
        } else {
            for step in bug.body.reproduction_steps.iter().take(6) {
                lines.push(Line::from(format!("- {}", short_text(step, 88))));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Automation",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(
            "Queue execution preview only; use attempt commands to record work.",
        ));
        lines
    } else {
        vec![Line::from(Span::styled(
            "No bug selected.",
            Style::default().fg(Color::DarkGray),
        ))]
    };
    f.render_widget(
        Paragraph::new(inspector_lines)
            .block(
                Block::default()
                    .title(" [ Inspector ] ")
                    .borders(Borders::ALL)
                    .border_style(focus::border_style(app, PaneId::BugsInspector)),
            )
            .wrap(Wrap { trim: false }),
        columns[2],
    );
}
