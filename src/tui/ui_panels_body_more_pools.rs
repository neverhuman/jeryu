use super::*;
use ratatui::widgets::{Cell, Row, Table};

// ---------------------------------------------------------------------------
// Tab 6 — Pools
// ---------------------------------------------------------------------------

pub(crate) fn draw_pools_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let has_nodes = !app.state.remote_nodes.is_empty();

    // Vertical split: top = pool list+detail, bottom = nodes sub-panel (when present).
    let rows = if has_nodes {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(7)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(100)])
            .split(area)
    };

    draw_pools_section(f, app, rows[0]);

    if has_nodes {
        draw_nodes_panel(f, app, rows[1]);
    }
}

// ---------------------------------------------------------------------------
// Pools section (list + detail)
// ---------------------------------------------------------------------------

fn draw_pools_section(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    focus::register_pane(app, PaneId::PoolsList, cols[0]);
    focus::register_pane(app, PaneId::PoolsDetail, cols[1]);
    focus::register_drill_esc_hotspot(app, PaneId::PoolsList, cols[0]);
    focus::register_drill_esc_hotspot(app, PaneId::PoolsDetail, cols[1]);

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

    let list_chrome = focus::pane_chrome(app, PaneId::PoolsList);
    let pools_title = if app.state.pool_sync_error.is_some() {
        list_chrome.title(&format!(
            "Runner Pools ({} cached) sync warning",
            app.state.pools.len()
        ))
    } else {
        list_chrome.title(&format!("Runner Pools ({})", app.state.pools.len()))
    };

    let list = List::new(items).block(
        Block::default()
            .title(pools_title)
            .borders(Borders::ALL)
            .border_style(list_chrome.border_style),
    );
    f.render_widget(list, cols[0]);

    // Right: pool detail
    let detail_body = if let Some(pool) = app.state.pools.get(app.selected_pool_index) {
        let backend_label = if pool.backend_type == "docker-remote" {
            if let Some(alias) = &pool.cluster_alias {
                format!("docker-remote ({})", alias)
            } else {
                "docker-remote".to_string()
            }
        } else {
            pool.backend_type.clone()
        };
        format!(
            "  Name:      {}\n  Status:    {}\n  Backend:   {}\n  Min Warm:  {} / Max: {}\n\n  [p] Toggle pause/resume",
            pool.name,
            if pool.paused { "[PAUSED]" } else { "[ACTIVE]" },
            backend_label,
            pool.min_warm,
            pool.max_managers,
        )
    } else if app.state.pool_sync_error.is_some() {
        "  No cached pool selected.".to_string()
    } else {
        "  No pool selected.".to_string()
    };
    let detail = if let Some(error) = app.state.pool_sync_error.as_deref() {
        format!("\n  Pool sync warning: {error}\n\n{detail_body}")
    } else {
        format!("\n{detail_body}")
    };

    let detail_chrome = focus::pane_chrome(app, PaneId::PoolsDetail);
    f.render_widget(
        Paragraph::new(detail)
            .block(
                Block::default()
                    .title(detail_chrome.title("Pool Detail"))
                    .borders(Borders::ALL)
                    .border_style(detail_chrome.border_style),
            )
            .wrap(Wrap { trim: false }),
        cols[1],
    );
}

// ---------------------------------------------------------------------------
// Remote nodes sub-panel
// ---------------------------------------------------------------------------

fn draw_nodes_panel(f: &mut Frame, app: &App, area: Rect) {
    let nodes = &app.state.remote_nodes;

    let rows: Vec<Row> = nodes
        .iter()
        .map(|n| {
            let status_cell = if !n.enabled {
                Cell::from("MAINT").style(Style::default().fg(Color::DarkGray))
            } else {
                match n.reachable {
                    Some(true) => Cell::from("OK   ").style(Style::default().fg(Color::Green)),
                    Some(false) => Cell::from("DOWN ").style(Style::default().fg(Color::Red)),
                    None => Cell::from("--   ").style(Style::default().fg(Color::DarkGray)),
                }
            };

            let mgr_str = format!("{}/{}", n.active_managers, n.max_managers);
            let mgr_cell = if n.active_managers == n.max_managers {
                Cell::from(mgr_str).style(Style::default().fg(Color::Yellow))
            } else {
                Cell::from(mgr_str)
            };

            let storage_cell = match n.storage_used_gb {
                Some(used) => {
                    let pct = (used / n.storage_limit_gb * 100.0) as u8;
                    let label = format!("{used:.0}/{:.0}GB {pct}%", n.storage_limit_gb);
                    let style = if pct >= 95 {
                        Style::default().fg(Color::Red)
                    } else if pct >= 80 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Green)
                    };
                    Cell::from(label).style(style)
                }
                None => Cell::from(format!("--/{:.0}GB", n.storage_limit_gb))
                    .style(Style::default().fg(Color::DarkGray)),
            };

            let last_contact = n.last_contact_at.as_deref().unwrap_or("never");
            // Shorten timestamp to just HH:MM:SS or relative.
            let contact_display = last_contact
                .split('T')
                .nth(1)
                .and_then(|t| t.get(..8))
                .unwrap_or(last_contact);

            Row::new(vec![
                Cell::from(n.alias.clone()),
                status_cell,
                mgr_cell,
                storage_cell,
                Cell::from(contact_display.to_string()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(14),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(16),
        Constraint::Length(12),
    ];

    let header = Row::new(vec![
        Cell::from("NODE").style(Style::default().fg(Color::Cyan)),
        Cell::from("STATE").style(Style::default().fg(Color::Cyan)),
        Cell::from("MGRS").style(Style::default().fg(Color::Cyan)),
        Cell::from("STORAGE").style(Style::default().fg(Color::Cyan)),
        Cell::from("CONTACT").style(Style::default().fg(Color::Cyan)),
    ])
    .height(1)
    .bottom_margin(0);

    let node_count = nodes.len();
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(format!(" Remote Nodes ({node_count}) "))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::BOLD));

    f.render_widget(table, area);
}
