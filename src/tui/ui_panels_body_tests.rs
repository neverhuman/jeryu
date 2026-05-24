use super::*;

// ---------------------------------------------------------------------------
// Tab 6 — Tests
// ---------------------------------------------------------------------------

pub(crate) fn draw_tests_tab(f: &mut Frame, app: &mut App, area: Rect) {
    // Optional VRC plan banner + Witness graph summary across the top.
    let vrc_visible = app.state.vrc.loaded && !app.state.vrc.plan.mode.is_empty();
    let witness_visible = app.state.witness.loaded && app.state.witness.crate_count > 0;
    let banner_height = match (vrc_visible, witness_visible) {
        (true, true) => 6,
        (true, false) | (false, true) => 3,
        (false, false) => 0,
    };
    let (vrc_area, witness_area, body_area) = if banner_height > 0 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(banner_height), Constraint::Min(8)])
            .split(area);
        let banner = rows[0];
        if vrc_visible && witness_visible {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(3)])
                .split(banner);
            (Some(split[0]), Some(split[1]), rows[1])
        } else if vrc_visible {
            (Some(banner), None, rows[1])
        } else {
            (None, Some(banner), rows[1])
        }
    } else {
        (None, None, area)
    };
    if let Some(banner) = vrc_area {
        draw_vrc_banner(f, app, banner);
    }
    if let Some(banner) = witness_area {
        draw_witness_summary(f, app, banner);
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(body_area);

    focus::register_pane(app, PaneId::TestsBottlenecks, chunks[0]);
    focus::register_pane(app, PaneId::TestsHistory, chunks[1]);
    focus::register_drill_esc_hotspot(app, PaneId::TestsBottlenecks, chunks[0]);
    focus::register_drill_esc_hotspot(app, PaneId::TestsHistory, chunks[1]);

    let (bottlenecks, label) = match app.test_view_mode {
        crate::tui::app::TestViewMode::Average => (&app.state.test_bottlenecks_avg, "Average"),
        crate::tui::app::TestViewMode::Latest => (&app.state.test_bottlenecks_latest, "Latest"),
    };

    let items: Vec<ListItem> = bottlenecks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let color = if i == app.selected_test_index {
                Color::Black
            } else if match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => b.avg_duration_ms > 300_000.0,
                crate::tui::app::TestViewMode::Latest => b.latest_duration_ms > 300_000,
            } {
                Color::Red
            } else if match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => b.avg_duration_ms > 60_000.0,
                crate::tui::app::TestViewMode::Latest => b.latest_duration_ms > 60_000,
            } {
                Color::Yellow
            } else {
                Color::Green
            };

            let bg = if i == app.selected_test_index {
                Color::Cyan
            } else {
                Color::Reset
            };

            let dur_text = match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => {
                    format!("{:.1}s", b.avg_duration_ms / 1000.0)
                }
                crate::tui::app::TestViewMode::Latest => {
                    format!("{:.1}s", b.latest_duration_ms as f64 / 1000.0)
                }
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<10} ", dur_text),
                    Style::default().fg(color).bg(bg),
                ),
                Span::styled(
                    format!("({:02}x) ", b.count),
                    Style::default().fg(Color::DarkGray).bg(bg),
                ),
                Span::styled(
                    b.test_name.clone(),
                    Style::default()
                        .fg(if i == app.selected_test_index {
                            Color::Black
                        } else {
                            Color::White
                        })
                        .bg(bg),
                ),
            ]))
        })
        .collect();

    let bottlenecks_chrome = focus::pane_chrome(app, PaneId::TestsBottlenecks);
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(bottlenecks_chrome.title(&format!("Bottlenecks ({}) - 'v' to toggle", label)))
            .border_style(bottlenecks_chrome.border_style),
    );
    f.render_widget(list, chunks[0]);

    let history_chrome = focus::pane_chrome(app, PaneId::TestsHistory);
    let history_block = Block::default()
        .borders(Borders::ALL)
        .title(history_chrome.title("History Drill-Down - Enter to load"))
        .border_style(history_chrome.border_style);

    if let Some(hist) = &app.selected_test_history {
        let h_items: Vec<ListItem> = hist
            .iter()
            .map(|h| {
                let color = match h.status.as_str() {
                    "success" | "passed" => Color::Green,
                    "failed" => Color::Red,
                    _ => Color::Yellow,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(
                            "{:<15} ",
                            h.created_at.split('T').next().unwrap_or(&h.created_at)
                        ),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(format!("{:<8} ", h.status), Style::default().fg(color)),
                    Span::styled(
                        format!("{:.1}s", h.duration_ms as f64 / 1000.0),
                        Style::default().fg(Color::White),
                    ),
                ]))
            })
            .collect();
        f.render_widget(List::new(h_items).block(history_block), chunks[1]);
    } else {
        f.render_widget(
            Paragraph::new("\n  Choose a test and press [Enter] to view execution history.")
                .block(history_block)
                .style(Style::default().fg(Color::DarkGray)),
            chunks[1],
        );
    }
}

fn draw_witness_summary(f: &mut Frame, app: &App, area: Rect) {
    let w = &app.state.witness;
    let largest = w
        .largest_crate
        .as_ref()
        .map(|(name, n)| format!("largest: {name} ({n} items)"))
        .unwrap_or_else(|| "no pub_items".into());
    let spans = vec![
        Span::styled(
            "  Witness ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("crates:{}  pub_items:{}  ", w.crate_count, w.pub_item_count),
            Style::default().fg(Color::White),
        ),
        Span::styled(largest, Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("   at: {}", w.generated_at),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_vrc_banner(f: &mut Frame, app: &App, area: Rect) {
    let plan = &app.state.vrc.plan;
    let mode_color = match plan.mode.as_str() {
        "full" => Color::Cyan,
        "selected" => Color::Green,
        "none" => Color::DarkGray,
        _ => Color::Yellow,
    };
    let mut spans: Vec<Span> = vec![
        Span::styled(
            "  VRC plan ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("mode={} ", plan.mode),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {}", plan.reason),
            Style::default().fg(Color::Gray),
        ),
    ];
    if !plan.selected_tests.is_empty() || !plan.skipped_tests.is_empty() {
        spans.push(Span::styled(
            format!(
                "   selected:{}  skipped:{}",
                plan.selected_tests.len(),
                plan.skipped_tests.len()
            ),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if let Some(conf) = plan.confidence {
        spans.push(Span::styled(
            format!("  conf:{:.0}%", conf * 100.0),
            Style::default().fg(Color::Cyan),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}
