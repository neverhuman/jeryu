use super::*;

// ---------------------------------------------------------------------------
// Audit ledger view (Evidence tab alternate mode)
// ---------------------------------------------------------------------------

pub(crate) fn draw_audit_ledger(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" [ Audit Ledger — 'a': capsule view ] ")
        .borders(Borders::ALL)
        .border_style(focus::border_style(app, PaneId::EvidenceDetail));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let tag_color = |ev_type: &str| -> Color {
        if ev_type.contains("cache") {
            Color::Blue
        } else if ev_type.contains("release") {
            Color::Magenta
        } else if ev_type.contains("secret") {
            Color::Yellow
        } else if ev_type.contains("agent") || ev_type.contains("capability") {
            Color::Cyan
        } else if ev_type.contains("job") {
            Color::Green
        } else {
            Color::Gray
        }
    };

    let items: Vec<Line> = app
        .state
        .recent_audit_events
        .iter()
        .take(inner.height as usize)
        .map(|ev| {
            let ts = ev.timestamp.get(..16).unwrap_or(&ev.timestamp);
            let tag = if ev.event_type.contains("cache") {
                "[CACHE]  "
            } else if ev.event_type.contains("release") {
                "[RELEASE]"
            } else if ev.event_type.contains("secret") {
                "[SECRET] "
            } else if ev.event_type.contains("agent") {
                "[AGENT]  "
            } else if ev.event_type.contains("job") {
                "[JOB]    "
            } else {
                "[EVENT]  "
            };
            let job_str = match ev.job_id {
                Some(id) => format!("job#{} ", id),
                None => String::new(),
            };
            Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} ", tag),
                    Style::default().fg(tag_color(&ev.event_type)),
                ),
                Span::styled(
                    format!("{:<20} ", short_text(&ev.event_type, 20)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{}{}", job_str, short_text(&ev.actor, 14)),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();

    if items.is_empty() {
        f.render_widget(
            Paragraph::new("\n  No audit events recorded yet.")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
    } else {
        f.render_widget(Paragraph::new(items), inner);
    }
}

// ---------------------------------------------------------------------------
// Tab 9 — Secrets
// ---------------------------------------------------------------------------

pub(crate) fn draw_secrets_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    focus::register_pane(app, PaneId::SecretsList, cols[0]);
    focus::register_pane(app, PaneId::SecretsDetail, cols[1]);

    let items: Vec<ListItem> = app
        .state
        .secret_audit_events
        .iter()
        .map(|ev| {
            let ts = ev.created_at.get(..16).unwrap_or(&ev.created_at);
            let status_col = match ev.status.as_str() {
                "ok" | "success" => Color::Green,
                "error" | "failed" => Color::Red,
                _ => Color::Yellow,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:<8} ", ev.action),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<8} ", ev.status),
                    Style::default().fg(status_col),
                ),
                Span::styled(&ev.repo_name, Style::default().fg(Color::White)),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(
                " [ Secret Audit Events ({}) ] ",
                app.state.secret_audit_events.len()
            ))
            .borders(Borders::ALL)
            .border_style(focus::border_style(app, PaneId::SecretsList)),
    );
    f.render_widget(list, cols[0]);

    f.render_widget(
        Paragraph::new("\n  Vault integration active.\n\n  Events appear here as secrets\n  are rotated, fetched, or revoked.\n\n  [RISK] = Security event requiring review.")
            .block(
            Block::default()
                .title(" [ Vault Status ] ")
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::SecretsDetail)),
            )
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        cols[1],
    );
}

// ---------------------------------------------------------------------------
// Tab 10 — Git
// ---------------------------------------------------------------------------

pub(crate) fn draw_git_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let rows: Vec<ListItem> = app
        .state
        .recent_git_events
        .iter()
        .map(|event| {
            let ts = event.created_at.get(..16).unwrap_or(&event.created_at);
            let status = if event.exit_code == 0 {
                "success"
            } else {
                "failed"
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", ts), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:<12} ", event.command_class),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("{:<5} ", status),
                    Style::default().fg(status_color(status)),
                ),
                Span::styled(
                    format!("{:<7} ", event.mirror_status),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    event.argv_redacted.clone(),
                    Style::default().fg(Color::White),
                ),
            ]))
        })
        .collect();

    focus::register_pane(app, PaneId::GitLedger, area);

    let body = if rows.is_empty() {
        List::new(vec![ListItem::new("  No git commands recorded yet.")])
    } else {
        List::new(rows)
    };

    f.render_widget(
        body.block(
            Block::default()
                .title(format!(
                    " [ Git Command Ledger ({}) ] ",
                    app.state.recent_git_events.len()
                ))
                .borders(Borders::ALL)
                .border_style(focus::border_style(app, PaneId::GitLedger)),
        ),
        area,
    );
}
