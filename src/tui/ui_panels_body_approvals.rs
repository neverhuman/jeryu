//! Owner: Interactive TUI subsystem — Approvals tab
//! Proof: `cargo nextest run -p jeryu -- tui::ui_panels_body_approvals`
//! Invariants: Rendering is read-only; approval action is dispatched via action_registry.

use super::*;

pub(crate) fn draw_approvals_tab(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    let queue = &app.state.approvals_queue;
    let selected = app
        .selected_approval_index
        .min(queue.len().saturating_sub(1));

    let items: Vec<ListItem> = queue
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                Style::default()
            };
            let tier_color = match p.risk_tier {
                0 => Color::Gray,
                1 => Color::Green,
                2 => Color::Cyan,
                3 => Color::Yellow,
                4 => Color::Red,
                _ => Color::Gray,
            };
            let ci_color = match p.ci_status.as_str() {
                "green" | "success" | "pass" => Color::Green,
                "running" | "pending" => Color::Blue,
                "failed" | "fail" | "error" => Color::Red,
                _ => Color::Yellow,
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("#{:<5} ", p.pr_number),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("T{} ", p.risk_tier),
                    Style::default().fg(tier_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<8} ", p.ci_status),
                    Style::default().fg(ci_color),
                ),
                Span::styled(
                    format!("{:<12} ", short_text(&p.agent_id, 12)),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(format!("{} ", p.age), Style::default().fg(Color::DarkGray)),
                Span::styled(short_text(&p.title, 36), style),
            ]))
        })
        .collect();

    let left = List::new(items).block(
        Block::default()
            .title(format!(" [ Awaiting approval ({}) ] ", queue.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(left, cols[0]);

    // Right: detail card for the selected PR.
    let right_block = Block::default()
        .title(" [ Inspector ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let right_inner = right_block.inner(cols[1]);
    f.render_widget(right_block, cols[1]);

    let mut lines: Vec<Line> = Vec::new();
    if queue.is_empty() {
        lines.push(Line::from(Span::styled(
            "\n  No PRs are awaiting approval.\n  Open a draft PR with `jeryu agent submit`.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let p = &queue[selected];
        lines.push(Line::from(vec![
            Span::styled("PR #", Style::default().fg(Color::Gray)),
            Span::styled(
                p.pr_number.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(p.title.clone()));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("agent: ", Style::default().fg(Color::Gray)),
            Span::raw(p.agent_id.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("tier:  ", Style::default().fg(Color::Gray)),
            Span::raw(format!("T{}", p.risk_tier)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("ci:    ", Style::default().fg(Color::Gray)),
            Span::raw(p.ci_status.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("age:   ", Style::default().fg(Color::Gray)),
            Span::raw(p.age.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("sha:   ", Style::default().fg(Color::Gray)),
            Span::raw(short_text(&p.head_sha, 12)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Actions:",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::UNDERLINED),
        )));
        lines.push(Line::from(vec![
            Span::styled("  ^K approve         ", Style::default().fg(Color::Green)),
            Span::styled(
                "jeryu release approve --pr N",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ^K request changes ", Style::default().fg(Color::Yellow)),
            Span::styled(
                "post a structured review",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ^K open in browser ", Style::default().fg(Color::Blue)),
            Span::styled("gh pr view --web", Style::default().fg(Color::DarkGray)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), right_inner);
}
