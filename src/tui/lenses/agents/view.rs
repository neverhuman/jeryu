//! Owner: Interactive TUI subsystem - Agents lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::agents::view`
//! Invariants: Pure draw. Reads `AgentsLensInput`; never touches DB, GitLab,
//!             Docker, Vault, filesystem, MCP, or network during render.
//!             Renders the agent fleet posture: a header summary, a metrics
//!             table (active / blocked / grants / can-code) with severity
//!             coloring, and a footer carrying the event cursor + key hints.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::AgentsLensInput;

pub fn draw(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / fleet summary
            Constraint::Min(0),    // metrics table
            Constraint::Length(3), // footer / cursor + key hints
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let text = format!(
        "Agents — {} active · {} blocked · {} grants",
        input.active_agents, input.blocked_agents, input.active_grants,
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Agents — Fleet "),
        ),
        area,
    );
}

/// A `(value, style)` pair for the right-hand column of a metric row.
fn count_cell(count: u32, alert: bool) -> Cell<'static> {
    let style = if alert {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    Cell::from(Span::styled(count.to_string(), style))
}

fn draw_body(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    // Blocked agents escalate to red — a stuck agent is burning a slot and a
    // grant while making no progress.
    let blocked_cell = if input.has_blocked() {
        Cell::from(Span::styled(
            input.blocked_agents.to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Cell::from(Span::styled("0", Style::default().fg(Color::Green)))
    };

    let (can_code_text, can_code_style) = if input.agents_can_code {
        ("yes", Style::default().fg(Color::Green))
    } else {
        (
            "NO",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    };

    let header = Row::new(vec![Cell::from("METRIC"), Cell::from("VALUE")])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows = vec![
        Row::new(vec![
            Cell::from("Active agents"),
            count_cell(input.active_agents, false),
        ]),
        Row::new(vec![Cell::from("Blocked agents"), blocked_cell]),
        Row::new(vec![
            Cell::from("Active grants"),
            // Grants are informational — bold them only when nonzero so the
            // pane reads cleanly when the fleet is parked.
            count_cell(input.active_grants, false),
        ]),
        Row::new(vec![
            Cell::from("Can code"),
            Cell::from(Span::styled(can_code_text, can_code_style)),
        ]),
    ];

    let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(8)])
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Lifecycle — {} ", input.fleet_status())),
        );

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let mut spans = vec![Span::raw(format!("cursor={}", input.event_cursor))];

    // Surface a freeze or a block inline so the operator sees why the fleet is
    // not advancing before reaching for an action.
    if !input.agents_can_code {
        spans.push(Span::styled(
            " · ❄ code frozen",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    } else if input.has_blocked() {
        spans.push(Span::styled(
            format!(" · ⚠ {} blocked", input.blocked_agents),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::raw(" · Keys: a actions · x explain · ? help"));

    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(input: &AgentsLensInput, w: u16, h: u16) -> String {
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
        let input = AgentsLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(&input, 80, 24);
        assert!(out.contains("Agents"));
        assert!(out.contains("active"));
        assert!(out.contains("Lifecycle"));
        assert!(out.contains("Can code"));
        assert!(out.contains("cursor=0"));
        assert!(out.contains("actions"));
    }

    #[test]
    fn renders_default_at_120x36() {
        let input = AgentsLensInput::from_read_model(&TuiReadModel::default());
        // Must not panic at the larger geometry.
        let _ = ink(&input, 120, 36);
    }

    #[test]
    fn renders_blocked_and_frozen_fleet() {
        let mut model = TuiReadModel::default();
        model.mission.active_agents = 4;
        model.mission.blocked_agents = 2;
        model.mission.active_grants = 7;
        model.mission.agents_can_code = false;
        let input = AgentsLensInput::from_read_model(&model);
        let out = ink(&input, 120, 36);
        assert!(out.contains("2 blocked"));
        assert!(out.contains("7 grants"));
        // Code freeze surfaces in the table value and the footer alert.
        assert!(out.contains("NO"));
        assert!(out.contains("frozen"));
        assert!(out.contains("FROZEN"));
    }

    #[test]
    fn renders_cursor_in_footer() {
        let model = TuiReadModel {
            event_cursor: 9001,
            ..Default::default()
        };
        let input = AgentsLensInput::from_read_model(&model);
        let out = ink(&input, 80, 24);
        assert!(out.contains("cursor=9001"));
    }
}
