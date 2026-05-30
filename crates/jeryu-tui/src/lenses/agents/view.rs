//! Agents lens view.
//!
//! Invariants: pure draw. Reads [`AgentsLensInput`]; no backend I/O. Renders
//! the agent fleet: a posture header, a per-session table (SESSION/STATUS/TASK/
//! BRANCH/GRANTS, status colored by lifecycle), and a footer carrying the
//! cursor plus a freeze/block alert.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::AgentStatus;

use super::data::{AgentRow, AgentsLensInput};

pub fn draw(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / fleet summary
            Constraint::Min(0),    // session table
            Constraint::Length(3), // footer / cursor + alert
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

fn status_style(status: AgentStatus) -> Style {
    match status {
        AgentStatus::Active => Style::default().fg(Color::Green),
        AgentStatus::Blocked => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        AgentStatus::Idle => Style::default().fg(Color::Gray),
        AgentStatus::Done => Style::default().fg(Color::DarkGray),
    }
}

fn draw_body(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    if input.rows.is_empty() {
        let (can_code_text, can_code_style) = if input.agents_can_code {
            ("yes", Style::default().fg(Color::Green))
        } else {
            (
                "NO",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        };
        let lines = vec![
            Line::from("No live agent sessions."),
            Line::from(vec![
                Span::raw("Can code: "),
                Span::styled(can_code_text, can_code_style),
            ]),
            Line::from(format!("Active grants: {}", input.active_grants)),
        ];
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Lifecycle — {} ", input.fleet_status())),
            ),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("SESSION"),
        Cell::from("STATUS"),
        Cell::from("TASK"),
        Cell::from("BRANCH"),
        Cell::from("GRANTS"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input.rows.iter().map(session_row).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(9),
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Lifecycle — {} ", input.fleet_status())),
    );

    f.render_widget(table, area);
}

fn session_row(r: &AgentRow) -> Row<'_> {
    Row::new(vec![
        Cell::from(r.label.clone()),
        Cell::from(Span::styled(
            r.status.label().to_string(),
            status_style(r.status),
        )),
        Cell::from(r.current_task.clone().unwrap_or_else(|| "—".into())),
        Cell::from(r.branch.clone().unwrap_or_else(|| "—".into())),
        Cell::from(r.grants.to_string()),
    ])
}

fn draw_footer(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let mut spans = vec![Span::raw(format!("cursor={}", input.event_cursor))];
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
    use jeryu_readmodel::{TuiReadModel, sample_read_model};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, input: &AgentsLensInput) -> String {
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
    fn renders_empty_at_80x24() {
        let input = AgentsLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Agents"));
        assert!(out.contains("Lifecycle"));
        assert!(out.contains("No live agent sessions."));
        assert!(out.contains("cursor=0"));
    }

    #[test]
    fn renders_sessions_at_120x36() {
        let input = AgentsLensInput::from_read_model(&sample_read_model());
        let out = ink(120, 36, &input);
        assert!(out.contains("agent-wrath-17"));
        assert!(out.contains("agent-storm-04"));
        assert!(out.contains("blocked"));
        assert!(out.contains("1 blocked"));
        assert!(out.contains("feat/approvals"));
    }

    #[test]
    fn renders_frozen_fleet() {
        let mut model = TuiReadModel::default();
        model.mission.active_agents = 3;
        model.mission.agents_can_code = false;
        let input = AgentsLensInput::from_read_model(&model);
        let out = ink(120, 36, &input);
        assert!(out.contains("FROZEN"));
        assert!(out.contains("frozen"));
        assert!(out.contains("NO"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = AgentsLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
