use super::*;

// ---------------------------------------------------------------------------
// Event console (bottom strip above footer)
// ---------------------------------------------------------------------------

pub(crate) fn draw_event_console(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Events ── Ctrl-K: command palette  /: search  ?: help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build ticker line from recent events (scrolling right-to-left)
    let mut ticker_spans: Vec<Span> = Vec::new();
    let _now = chrono::Utc::now();

    // Collect event entries
    let events: Vec<(&str, &str, Color, &str)> = app
        .state
        .recent_jobs
        .iter()
        .take(20)
        .map(|job| {
            let ts = job.received_at.get(11..19).unwrap_or("--:--:--");
            let (badge, color) = status_badge(&job.status);
            let name = job.job_name.as_deref().unwrap_or("job");
            (ts, badge, color, name)
        })
        .collect();

    if events.is_empty() {
        let p = Paragraph::new(Span::styled(
            "  No events yet. Events appear here as jobs run.",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(p, inner);
        return;
    }

    // Build a single scrolling line
    for (ts, badge, color, name) in &events {
        ticker_spans.push(Span::styled(
            format!(" {ts} "),
            Style::default().fg(Color::DarkGray),
        ));
        ticker_spans.push(Span::styled(
            format!("[{badge}]"),
            Style::default().fg(*color).add_modifier(Modifier::BOLD),
        ));
        ticker_spans.push(Span::styled(
            format!(" {name}  │"),
            Style::default().fg(Color::White),
        ));
    }

    // Scroll offset drives the horizontal shift
    let offset = (app.state.event_ticker_offset % (events.len() * 30 + 1)) as u16;

    let p = Paragraph::new(Line::from(ticker_spans)).scroll((0, offset));
    f.render_widget(p, inner);
}

// ---------------------------------------------------------------------------
// Footer / key hints
// ---------------------------------------------------------------------------

pub(crate) fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let help = if app.maximize_logs {
        " Esc:minimize  ↑↓:scroll  PgUp/Dn:jump  Home:top  G/End:bottom  q:quit"
    } else {
        match app.active_tab {
            ActiveTab::Jobs => {
                " f:freeze  n/N:runner  g:follow  r:requeue  d:remove  Enter:logs  /:search  ?:help  ^K:palette  q:quit"
            }
            ActiveTab::Tests => {
                " v:view-mode  Enter:history  ↑↓:move  /:search  ?:help  ^K:palette  q:quit"
            }
            ActiveTab::Workflow => {
                " ↑↓:phase  ←→:node  Tab:next  Enter:inspect  PgUp/Dn:pan50%  []:pan-h  f:follow  Home/End  ?:help  ^K:palette  q:quit"
            }
            ActiveTab::Mission => {
                " .:next-action  ?:explain  Enter:inspect  /:search  ^K:palette  q:quit"
            }
            ActiveTab::Agents => {
                " Enter:inspect  p:preview  x:execute  ?:explain  /:search  ^K:palette  q:quit"
            }
            ActiveTab::Evidence => {
                " a:toggle-view  Enter:inspect  /:search  ?:help  ^K:palette  q:quit"
            }
            ActiveTab::Pools => {
                " p:pause/resume  Enter:inspect  /:search  ?:help  ^K:palette  q:quit"
            }
            _ => {
                " Enter:inspect  Tab:cycle  1-0:tab  ↑↓:move  /:search  ?:help  ^K:palette  q:quit"
            }
        }
    };
    let p = Paragraph::new(help)
        .block(Block::default().borders(Borders::TOP))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}
