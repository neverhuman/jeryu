use super::*;

pub(super) fn render_breakdown_panel(
    f: &mut Frame,
    area: Rect,
    dimensions: &[crate::tui::jankurai::JankuraiDimension],
    inner: Rect,
) {
    if dimensions.is_empty() {
        f.render_widget(
            Paragraph::new("  No dimension breakdown available")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
    } else {
        let lines = dimensions
            .iter()
            .map(|dimension| {
                let notes = if dimension.notes.is_empty() {
                    String::new()
                } else {
                    format!(" notes: {}", short_text(&dimension.notes.join("; "), 40))
                };
                Line::from(vec![
                    Span::styled(
                        format!("{:>3} ", dimension.score),
                        Style::default().fg(if dimension.score >= 90 {
                            Color::Green
                        } else if dimension.score >= 75 {
                            Color::Yellow
                        } else {
                            Color::Red
                        }),
                    ),
                    Span::styled(
                        format!("w{:>2} ", dimension.weight),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(
                        short_text(
                            &dimension.name,
                            inner.width.saturating_sub(16) as usize,
                        ),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(notes, Style::default().fg(Color::DarkGray)),
                ])
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            inner,
        );
    }
    let _ = area; // layout area passed for potential future use
}

pub(super) fn render_issues_panel(
    f: &mut Frame,
    area: Rect,
    entries: &[crate::tui::jankurai::JankuraiEntry],
    selected_index: usize,
    inner: Rect,
) {
    let (visible_start, visible_end) =
        super::ui_panels_jankurai_helpers::visible_entry_window(
            entries.len(),
            selected_index,
            inner.height as usize,
        );
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .skip(visible_start)
        .take(visible_end.saturating_sub(visible_start))
        .enumerate()
        .map(|(visible_index, (entry_index, entry))| {
            let index = visible_start + visible_index;
            debug_assert_eq!(index, entry_index);
            let selected = index == selected_index;
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let (badge, badge_color) = match entry.kind {
                crate::tui::jankurai::JankuraiEntryKind::Cap => ("CAP", Color::Magenta),
                crate::tui::jankurai::JankuraiEntryKind::Finding => {
                    match entry.severity.as_deref() {
                        Some("high") => ("HIGH", Color::Red),
                        Some("medium") => ("MED", Color::Yellow),
                        Some("low") => ("LOW", Color::Green),
                        _ => ("INFO", Color::Gray),
                    }
                }
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {:<5} ", badge),
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "{:<18} ",
                        short_text(entry.path.as_deref().unwrap_or(""), 18)
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    short_text(
                        entry.problem.as_deref().unwrap_or(&entry.label),
                        inner.width.saturating_sub(32) as usize,
                    ),
                    Style::default().fg(Color::White),
                ),
            ]);
            ListItem::new(line).style(style)
        })
        .collect();

    if items.is_empty() {
        f.render_widget(
            Paragraph::new("  No caps or findings recorded.")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
    } else {
        f.render_widget(List::new(items), inner);
    }
    let _ = area;
}

pub(super) fn render_entry_detail(
    f: &mut Frame,
    _area: Rect,
    entry: &crate::tui::jankurai::JankuraiEntry,
    inner: Rect,
) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("kind:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                match entry.kind {
                    crate::tui::jankurai::JankuraiEntryKind::Cap => "cap",
                    crate::tui::jankurai::JankuraiEntryKind::Finding => "finding",
                },
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("rule:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                entry.rule.as_deref().unwrap_or("n/a"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("path:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                entry.path.as_deref().unwrap_or("n/a"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("lane:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                entry.lane.as_deref().unwrap_or("n/a"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("owner:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                entry.owner.as_deref().unwrap_or("n/a"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("severity:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                entry.severity.as_deref().unwrap_or("n/a"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("hardness:", Style::default().fg(Color::DarkGray)),
            Span::styled(
                entry.hardness.as_deref().unwrap_or("n/a"),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(Span::styled(
            "────────────────────────",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(vec![
            Span::styled("problem: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                short_text(
                    entry.problem.as_deref().unwrap_or("n/a"),
                    inner.width.saturating_sub(11) as usize,
                ),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("fix:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                short_text(
                    entry.suggested_fix.as_deref().unwrap_or("n/a"),
                    inner.width.saturating_sub(11) as usize,
                ),
                Style::default().fg(Color::Yellow),
            ),
        ]),
    ];

    if !entry.evidence.is_empty() {
        lines.push(Line::from(Span::styled(
            "evidence:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        for item in &entry.evidence {
            lines.push(Line::from(Span::styled(
                format!(
                    "  - {}",
                    short_text(item, inner.width.saturating_sub(6) as usize)
                ),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner,
    );
}
