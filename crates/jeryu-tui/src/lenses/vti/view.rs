//! Owner: Interactive TUI subsystem - VTI lens view
//! Proof: `cargo test -p jeryu-tui --lib lenses::vti::view`
//! Invariants: Pure draw. Reads `VtiLensInput`; never touches DB, forge,
//!             Docker, Vault, filesystem, MCP, or network during render.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::VtiLensInput;
use jeryu_readmodel::HealthLevel;

pub fn draw(f: &mut Frame, input: &VtiLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &VtiLensInput, area: Rect) {
    let text = format!(
        "VTI Tests — {} selector misses (24h) · {} failing",
        input.selector_misses_24h, input.failed_jobs
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" VTI — Test Impact "),
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

fn metric_row(label: &str, value: Span<'static>) -> Row<'static> {
    Row::new(vec![Cell::from(label.to_string()), Cell::from(value)])
}

fn draw_body(f: &mut Frame, input: &VtiLensInput, area: Rect) {
    let failed_value = if input.failed_jobs > 0 {
        Span::styled(
            input.failed_jobs.to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("0")
    };

    let posture = Span::styled(
        format!("{} {}", input.overall.glyph(), input.overall.label()),
        health_style(input.overall),
    );

    let rows = vec![
        metric_row("Running jobs", Span::raw(input.running_jobs.to_string())),
        metric_row("Failed jobs", failed_value),
        metric_row("Queued jobs", Span::raw(input.queued_jobs.to_string())),
        metric_row(
            "Selector misses (24h)",
            Span::raw(input.selector_misses_24h.to_string()),
        ),
        metric_row("Overall posture", posture),
    ];

    let table = Table::new(rows, [Constraint::Length(24), Constraint::Min(12)])
        .header(
            Row::new(vec![Cell::from("METRIC"), Cell::from("VALUE")])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Test Selection "),
        );

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &VtiLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: r run · s select · e evidence",
        input.event_cursor
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::TuiReadModel;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(input: &VtiLensInput, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
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
        let input = VtiLensInput::from_read_model(&TuiReadModel::default());
        let ink = render(&input, 80, 24);
        assert!(ink.contains("VTI"));
        assert!(ink.contains("selector misses"));
        assert!(ink.contains("Running jobs"));
        assert!(ink.contains("Overall posture"));
        assert!(ink.contains("HEALTHY"));
        assert!(ink.contains("Keys: r run"));
    }

    #[test]
    fn renders_failing_posture_at_120x36() {
        let mut model = TuiReadModel::default();
        model.mission.selector_misses_24h = 12;
        model.mission.running_jobs = 5;
        model.mission.failed_jobs = 3;
        model.mission.queued_jobs = 8;
        model.mission.overall = HealthLevel::Critical;
        let input = VtiLensInput::from_read_model(&model);
        let ink = render(&input, 120, 36);
        assert!(ink.contains("3 failing"));
        assert!(ink.contains("CRITICAL"));
        assert!(ink.contains("Queued jobs"));
    }

    #[test]
    fn footer_carries_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = VtiLensInput::from_read_model(&model);
        let ink = render(&input, 80, 24);
        assert!(ink.contains("cursor=42"));
    }
}
