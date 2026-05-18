//! Owner: Interactive TUI subsystem — action preview modal
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::action_preview`
//! Invariants: Preview modal is read-only; it never executes the action.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::api::actions::ActionPreview;
use crate::tui::theme::Theme;

/// Render a centered action preview modal over the current screen.
pub fn render_action_preview(
    f: &mut Frame,
    area: Rect,
    action_label: &str,
    preview: &ActionPreview,
    theme: &Theme,
) {
    // Center a modal at 60% width, 50% height
    let modal = centered_rect(60, 50, area);

    // Clear the background
    f.render_widget(Clear, modal);

    let risk_color = theme.status_color(preview.risk.label());
    let block = Block::default()
        .title(format!(" Action Preview — {} ", action_label))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(risk_color));

    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Summary + Risk
            Constraint::Length(4), // Will do
            Constraint::Length(3), // Will NOT
            Constraint::Length(2), // Grant + undo
            Constraint::Min(2),    // Confirmation
        ])
        .split(inner);

    // Summary + Risk
    let summary_lines = vec![
        Line::from(vec![
            Span::styled("  Risk: ", theme.muted()),
            Span::styled(preview.risk.label(), theme.bold(risk_color)),
            Span::styled(
                format!("   Effect: {}", preview.side_effect_class.label()),
                theme.secondary(),
            ),
        ]),
        Line::from(vec![
            Span::styled("  ", theme.muted()),
            Span::styled(&preview.summary, theme.primary()),
        ]),
    ];
    f.render_widget(Paragraph::new(summary_lines), sections[0]);

    // Will do
    let mut will_lines = vec![Line::from(Span::styled(
        "  Will:",
        theme.bold(theme.text_secondary),
    ))];
    for effect in preview.side_effects.iter().take(3) {
        will_lines.push(Line::from(vec![
            Span::styled("    • ", Style::default().fg(theme.running)),
            Span::styled(effect.clone(), theme.primary()),
        ]));
    }
    f.render_widget(Paragraph::new(will_lines), sections[1]);

    // Will NOT
    let mut wont_lines = vec![Line::from(Span::styled(
        "  Will NOT:",
        theme.bold(theme.text_secondary),
    ))];
    for wont in preview.will_not.iter().take(2) {
        wont_lines.push(Line::from(vec![
            Span::styled("    ✗ ", Style::default().fg(theme.skipped)),
            Span::styled(wont.clone(), theme.muted()),
        ]));
    }
    f.render_widget(Paragraph::new(wont_lines), sections[2]);

    // Grant + undo
    let grant_lines = vec![Line::from(vec![
        Span::styled("  Grant: ", theme.muted()),
        Span::styled(preview.required_grant.label(), theme.secondary()),
        Span::styled("   Undo: ", theme.muted()),
        Span::styled(
            preview.undo_action.as_deref().unwrap_or("n/a"),
            theme.secondary(),
        ),
    ])];
    f.render_widget(Paragraph::new(grant_lines), sections[3]);

    // Confirmation footer
    let confirm = preview
        .confirm_prompt
        .as_deref()
        .unwrap_or("[Enter] Execute   [Esc] Cancel");
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ", theme.muted()),
            Span::styled(confirm.to_string(), theme.bold(theme.border_accent)),
        ]))
        .wrap(Wrap { trim: true }),
        sections[4],
    );
}

fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vert[1]);
    horiz[1]
}
