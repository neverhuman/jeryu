//! Owner: Interactive TUI subsystem - Runners lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::runners::view`
//! Invariants: Pure draw. Reads RunnersLensInput; no I/O.
//!             Renders the live multinode runner grid (one row per node) with a
//!             fleet utilization header and an alert banner when any node is
//!             unreachable.

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
    let util = if input.total_runners > 0 {
        (input.active_runners as f64 / input.total_runners as f64 * 100.0).round() as u32
    } else {
        0
    };
    let text = format!(
        "Runners — {}/{} managers active ({}% utilization) · {} nodes",
        input.active_runners,
        input.total_runners,
        util,
        input.nodes.len()
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

fn node_status_style(row: &RunnerNodeRow) -> Style {
    match row.status() {
        "UNREACHABLE" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        "saturated" => Style::default().fg(Color::Yellow),
        "idle" => Style::default().fg(Color::Gray),
        "probing" => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Green),
    }
}

fn storage_cell(row: &RunnerNodeRow) -> String {
    match row.storage_used_gb {
        Some(used) => format!("{used:.0}/{:.0} GiB", row.storage_limit_gb),
        None => "—".to_string(),
    }
}

fn draw_node_grid(f: &mut Frame, input: &RunnersLensInput, area: Rect) {
    if input.nodes.is_empty() {
        f.render_widget(
            Paragraph::new(
                "No node telemetry yet. Live per-node runner health appears here once the \
                 fleet sync has probed the nodes (xbabe0/1/2/3).",
            )
            .block(Block::default().borders(Borders::ALL).title(" Nodes ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("NODE"),
        Cell::from("STATUS"),
        Cell::from("MANAGERS"),
        Cell::from("STORAGE"),
        Cell::from("LAST CONTACT"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .nodes
        .iter()
        .map(|n| {
            Row::new(vec![
                Cell::from(n.alias.clone()),
                Cell::from(Span::styled(n.status().to_string(), node_status_style(n))),
                Cell::from(format!("{}/{}", n.active_managers, n.max_managers)),
                Cell::from(storage_cell(n)),
                Cell::from(n.last_contact_at.clone().unwrap_or_else(|| "—".into())),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(16),
            Constraint::Min(12),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Nodes "));

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &RunnersLensInput, area: Rect) {
    let unreachable = input.unreachable_nodes();
    let line = if unreachable > 0 {
        Line::from(Span::styled(
            format!(
                "⚠ {unreachable} node(s) UNREACHABLE — capacity at risk. Keys: d drain · p pause · s scale · e evidence · ? help"
            ),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(
            "fleet healthy · Keys: d drain · p pause · s scale · e evidence · ? help".to_string(),
        )
    };
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::NodeSummary;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn input_with_nodes() -> RunnersLensInput {
        let nodes = vec![
            NodeSummary {
                alias: "xbabe0".into(),
                target: "deploy@xbabe0".into(),
                enabled: true,
                reachable: Some(true),
                active_managers: 3,
                max_managers: 4,
                storage_used_gb: Some(78.0),
                storage_limit_gb: 100.0,
                last_contact_at: Some("2m ago".into()),
            },
            NodeSummary {
                alias: "xbabe1".into(),
                target: "deploy@xbabe1".into(),
                enabled: true,
                reachable: Some(false),
                active_managers: 0,
                max_managers: 4,
                storage_used_gb: None,
                storage_limit_gb: 100.0,
                last_contact_at: Some("17m ago".into()),
            },
        ];
        RunnersLensInput::from_nodes(&nodes, 7)
    }

    #[test]
    fn renders_node_grid_with_aliases_and_alert() {
        let input = input_with_nodes();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("xbabe0"), "node alias must render");
        assert!(ink.contains("xbabe1"));
        assert!(
            ink.contains("UNREACHABLE"),
            "unreachable node must be flagged"
        );
        assert!(ink.contains("utilization"), "fleet header must render");
    }

    #[test]
    fn empty_nodes_render_placeholder_not_panic() {
        let input =
            RunnersLensInput::from_read_model(&crate::api::read_model::TuiReadModel::default());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Runners"));
    }

    #[test]
    fn renders_at_220x60() {
        let input = input_with_nodes();
        let backend = TestBackend::new(220, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
    }
}
