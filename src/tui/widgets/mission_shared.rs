//! Owner: Interactive TUI subsystem - reusable widget library
//! Proof: `cargo nextest run -p jeryu -- tui::widgets`
//! Invariants: Widgets are pure rendering functions; they never mutate control-plane state.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub(crate) struct MetricTile<'a> {
    pub title: &'a str,
    pub value: &'a str,
    pub detail: Option<&'a str>,
    pub color: Color,
}

pub(crate) fn render_metric_tile(
    f: &mut Frame,
    area: Rect,
    title: &str,
    value: &str,
    detail: Option<&str>,
    color: Color,
) {
    let mut lines = vec![Line::from(Span::styled(
        format!("  {value}"),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))];
    if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
        lines.push(Line::from(Span::styled(
            format!("  {detail}"),
            Style::default().fg(Color::White),
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(format!(" [ {title} ] "))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        ),
        area,
    );
}

pub(crate) fn render_metric_row(f: &mut Frame, area: Rect, tiles: &[MetricTile<'_>]) {
    let count = tiles.len().max(1);
    let constraints = vec![Constraint::Percentage((100 / count.max(1)) as u16); count];
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (col, tile) in cols.iter().zip(tiles.iter()) {
        render_metric_tile(f, *col, tile.title, tile.value, tile.detail, tile.color);
    }
}
