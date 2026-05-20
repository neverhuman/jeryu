use super::super::*;

// ---------------------------------------------------------------------------
// TUI v2 — Help overlay
// ---------------------------------------------------------------------------

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
        ActiveTab::Bugs => "Bugs",
        ActiveTab::LLMs => "LLMs",
        ActiveTab::Git => "Git",
        ActiveTab::Secrets => "Secrets",
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
