//! Owner: Interactive TUI subsystem - Cache lens view
//! Proof: `cargo test -p jeryu-tui --lib lenses::cache::view`
//! Invariants: Pure draw. Reads CacheLensInput; no I/O. Surfaces SmartCache
//!             health — hit ratio, taints, and the `system.cache` component
//!             status (colored by HealthLevel).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::HealthLevel;

use super::data::CacheLensInput;

pub fn draw(f: &mut Frame, input: &CacheLensInput, area: Rect) {
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

/// Color a component status by its HealthLevel (mirrors runners' status styling).
fn status_style(level: HealthLevel) -> Style {
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

/// A ten-cell text meter: `████████░░ 80%`.
fn ratio_bar(ratio: f64) -> String {
    let pct = (ratio.clamp(0.0, 1.0) * 100.0).round() as u32;
    let filled = ((pct + 5) / 10).min(10) as usize;
    let empty = 10 - filled;
    format!("{}{} {pct}%", "█".repeat(filled), "░".repeat(empty))
}

fn draw_header(f: &mut Frame, input: &CacheLensInput, area: Rect) {
    let pct = (input.cache_hit_ratio.clamp(0.0, 1.0) * 100.0).round() as u32;
    let text = format!(
        "Cache — {pct}% hit · {} taints · {}",
        input.active_taints,
        input.component_status.label(),
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Cache — SmartCache "),
        ),
        area,
    );
}

fn draw_body(f: &mut Frame, input: &CacheLensInput, area: Rect) {
    let latency = match input.component_latency_ms {
        Some(ms) => format!("{ms} ms"),
        None => "—".to_string(),
    };
    let detail = input
        .component_detail
        .clone()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "—".to_string());

    let status_cell = Cell::from(Span::styled(
        format!(
            "{} {}",
            input.component_status.glyph(),
            input.component_status.label()
        ),
        status_style(input.component_status),
    ));

    let rows = vec![
        Row::new(vec![
            Cell::from("Hit ratio"),
            Cell::from(ratio_bar(input.cache_hit_ratio)),
        ]),
        Row::new(vec![
            Cell::from("Active taints"),
            Cell::from(format!(
                "{} (total {})",
                input.active_taints, input.taint_count
            )),
        ]),
        Row::new(vec![
            Cell::from(format!("Component ({})", input.component_name)),
            status_cell,
        ]),
        Row::new(vec![Cell::from("Latency"), Cell::from(latency)]),
        Row::new(vec![Cell::from("Detail"), Cell::from(detail)]),
    ];

    let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(20)])
        .header(
            Row::new(vec![Cell::from("METRIC"), Cell::from("VALUE")])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().borders(Borders::ALL).title(" Health "));

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &CacheLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: g gc · e evidence · ? help",
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

    fn ink(width: u16, height: u16, input: &CacheLensInput) -> String {
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
    fn ratio_bar_renders_proportionally() {
        assert_eq!(ratio_bar(0.0), "░░░░░░░░░░ 0%");
        assert_eq!(ratio_bar(1.0), "██████████ 100%");
        assert!(ratio_bar(0.8).ends_with("80%"));
    }

    #[test]
    fn renders_at_80x24() {
        let input = CacheLensInput::from_read_model(&TuiReadModel::default());
        let s = ink(80, 24, &input);
        assert!(s.contains("Cache"));
        assert!(s.contains("Hit ratio"));
        assert!(s.contains("taints"));
        assert!(s.contains("cursor="));
    }

    #[test]
    fn renders_populated_at_120x36() {
        let mut model = TuiReadModel {
            event_cursor: 99,
            ..Default::default()
        };
        model.mission.cache_hit_ratio = 0.8;
        model.mission.active_taints = 3;
        model.mission.taint_count = 11;
        model.system.cache = jeryu_readmodel::ComponentHealth::ok("cache", 4);
        let input = CacheLensInput::from_read_model(&model);
        let s = ink(120, 36, &input);
        assert!(s.contains("80%"));
        assert!(s.contains("HEALTHY"));
        assert!(s.contains("cursor=99"));
    }
}
