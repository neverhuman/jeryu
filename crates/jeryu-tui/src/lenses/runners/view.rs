//! Runners lens view.
//!
//! Invariants: pure draw. Reads [`RunnersLensInput`]; no backend I/O. Renders a
//! fleet-utilization header, a per-node grid (NODE/STATUS/POOL/TAGS/CONTACT),
//! and an alert banner when any node is stuck.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::{RunnerNodeRow, RunnersLensInput};

pub fn draw(f: &mut Frame, input: &RunnersLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / fleet summary
            Constraint::Min(0),    // per-node grid
            Constraint::Length(3), // alert banner / footer
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_node_grid(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &RunnersLensInput, area: Rect) {
    let text = format!(
        "Runners — {}/{} active ({}% utilization) · {} paused · {} draining · {} nodes",
        input.active_runners,
        input.total_runners,
        input.utilization_pct(),
        input.paused_runners,
        input.draining_runners,
        input.nodes.len(),
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Runners — Fleet "),
        ),
        area,
    );
}

fn status_style(row: &RunnerNodeRow) -> Style {
    match row.status_word() {
        "STUCK" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        "busy" => Style::default().fg(Color::Yellow),
        "idle" => Style::default().fg(Color::Gray),
        "probing" => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Green),
    }
}

fn draw_node_grid(f: &mut Frame, input: &RunnersLensInput, area: Rect) {
    if input.nodes.is_empty() {
        f.render_widget(
            Paragraph::new(
                "No runner telemetry yet. Per-node fleet health appears here once the read \
                 model carries runner items.",
            )
            .block(Block::default().borders(Borders::ALL).title(" Nodes ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("NODE"),
        Cell::from("STATUS"),
        Cell::from("POOL"),
        Cell::from("TAGS"),
        Cell::from("CONTACT"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .nodes
        .iter()
        .map(|n| {
            Row::new(vec![
                Cell::from(n.label.clone()),
                Cell::from(Span::styled(n.status_word().to_string(), status_style(n))),
                Cell::from(n.pool.clone()),
                Cell::from(n.tags.join(",")),
                Cell::from(n.last_contact.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Min(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Nodes "));

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &RunnersLensInput, area: Rect) {
    let stuck = input.stuck_nodes();
    let line = if stuck > 0 {
        Line::from(Span::styled(
            format!(
                "⚠ {stuck} node(s) STUCK — capacity at risk · cursor={} · Keys: d drain · p pause · s scale",
                input.event_cursor
            ),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(format!(
            "fleet healthy · cursor={} · Keys: d drain · p pause · s scale · e evidence",
            input.event_cursor
        ))
    };
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{HealthLevel, RunnersItem, TuiReadModel, sample_read_model};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, input: &RunnersLensInput) -> String {
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
    fn renders_empty_fleet_at_80x24() {
        let input = RunnersLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Runners"));
        assert!(out.contains("utilization"));
        assert!(out.contains("No runner telemetry"));
        assert!(out.contains("fleet healthy"));
    }

    #[test]
    fn renders_node_grid_at_120x36() {
        let input = RunnersLensInput::from_read_model(&sample_read_model());
        let out = ink(120, 36, &input);
        assert!(out.contains("oci-runner-1"));
        assert!(out.contains("STATUS"));
        assert!(out.contains("75% utilization"));
        assert!(out.contains("online"));
    }

    #[test]
    fn renders_stuck_alert_banner() {
        let mut model = TuiReadModel::default();
        model.runners.items = vec![RunnersItem {
            id: "1".into(),
            label: "oci-runner-9".into(),
            runner_id: "r9".into(),
            pool: "trusted".into(),
            status: HealthLevel::Critical,
            tags: vec!["oci".into()],
            last_seen: None,
        }];
        let input = RunnersLensInput::from_read_model(&model);
        let out = ink(120, 36, &input);
        assert!(out.contains("STUCK"));
        assert!(out.contains("capacity at risk"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = RunnersLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
