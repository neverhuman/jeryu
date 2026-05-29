//! Owner: Interactive TUI subsystem - Source Doctor lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::source_doctor::view`
//! Invariants: Pure draw. Reads `SourceDoctorLensInput`; never touches DB,
//!             GitLab, Docker, Vault, filesystem, MCP, or network during
//!             render. Renders one diagnostic row per infra/config component
//!             (plus a runners fleet row) with HealthLevel-colored status.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::{ComponentRow, SourceDoctorLensInput};
use crate::api::entity::HealthLevel;

pub fn draw(f: &mut Frame, input: &SourceDoctorLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / health summary
            Constraint::Min(0),    // per-component diagnostic table
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_components(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &SourceDoctorLensInput, area: Rect) {
    let text = format!(
        "Source Doctor — {}/{} components healthy",
        input.healthy_count(),
        input.total_count(),
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Source Doctor "),
        ),
        area,
    );
}

fn health_style(level: HealthLevel) -> Style {
    match level {
        HealthLevel::Healthy => Style::default().fg(Color::Green),
        HealthLevel::Warning => Style::default().fg(Color::Yellow),
        HealthLevel::Degraded => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        HealthLevel::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        HealthLevel::Unknown => Style::default().fg(Color::DarkGray),
    }
}

fn draw_components(f: &mut Frame, input: &SourceDoctorLensInput, area: Rect) {
    if input.components.is_empty() {
        f.render_widget(
            Paragraph::new("No components reported by the read model yet.")
                .block(Block::default().borders(Borders::ALL).title(" Components ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("COMPONENT"),
        Cell::from("STATUS"),
        Cell::from("DETAIL"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .components
        .iter()
        .map(|c: &ComponentRow| {
            Row::new(vec![
                Cell::from(c.name.clone()),
                Cell::from(Span::styled(c.health.label(), health_style(c.health))),
                Cell::from(c.detail.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Components "));

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &SourceDoctorLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: r recheck · x explain · ? help",
        input.event_cursor
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .border_style(Style::default().add_modifier(Modifier::BOLD)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(width: u16, height: u16, input: &SourceDoctorLensInput) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, input, f.area())).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn renders_default_at_80x24() {
        let model = TuiReadModel::default();
        let input = SourceDoctorLensInput::from_read_model(&model);
        let buf = ink(80, 24, &input);
        assert!(!buf.trim().is_empty());
        assert!(buf.contains("Source Doctor"));
        assert!(buf.contains("Components"));
        assert!(buf.contains("gitlab"));
        assert!(buf.contains("runners"));
        assert!(buf.contains("recheck"));
    }

    #[test]
    fn renders_at_120x36_with_health_summary_and_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = SourceDoctorLensInput::from_read_model(&model);
        let buf = ink(120, 36, &input);
        assert!(buf.contains("components healthy"));
        assert!(buf.contains("cursor=42"));
    }
}
