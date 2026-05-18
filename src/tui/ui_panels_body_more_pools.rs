use super::*;

// ---------------------------------------------------------------------------
// Tab 6 — Pools
// ---------------------------------------------------------------------------

pub(crate) fn draw_pools_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    focus::register_focus_pane(app, PaneId::PoolsList, cols[0]);
    focus::register_focus_pane(app, PaneId::PoolsDetail, cols[1]);

    // Left: pools list
    let active = app.active_tab == ActiveTab::Pools;
    let items: Vec<ListItem> = app
        .state
        .pools
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let selected = active && i == app.selected_pool_index;
            let prefix = if selected { "> " } else { "  " };
            let state_badge = if p.paused {
                Span::styled("[PAUSED]", Style::default().fg(Color::Yellow))
            } else {
                Span::styled("[ACTIVE]", Style::default().fg(Color::Green))
            };
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan)),
                state_badge,
                Span::raw(format!(" {}", p.name)),
            ]);
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(focus::pane_block(
        app,
        PaneId::PoolsList,
        format!(" [ Runner Pools ({}) ] ", app.state.pools.len()),
    ));
    f.render_widget(list, cols[0]);

    // Right: pool detail
    let detail = if let Some(pool) = app.state.pools.get(app.selected_pool_index) {
        format!(
            "\n  Name:      {}\n  Status:    {}\n  Min Warm:  {}\n  Max:       {}\n\n  [p] Toggle pause/resume",
            pool.name,
            if pool.paused { "[PAUSED]" } else { "[ACTIVE]" },
            pool.min_warm,
            pool.max_managers,
        )
    } else {
        "\n  No pool selected.".to_string()
    };

    f.render_widget(
        Paragraph::new(detail)
            .block(focus::pane_block(
                app,
                PaneId::PoolsDetail,
                " [ Pool Detail ] ",
            ))
            .wrap(Wrap { trim: false }),
        cols[1],
    );
}
