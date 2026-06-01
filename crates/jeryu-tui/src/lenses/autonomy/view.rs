//! Owner: Interactive TUI subsystem - Autonomy lens view
//! Proof: `cargo test -p jeryu-tui --lib tui::lenses::autonomy::view`
//! Invariants: Pure draw. Reads `AutonomyLensInput`; never touches DB, GitLab,
//!             Docker, Vault, filesystem, MCP, or network during render.
//!             Renders the guardrail posture for autonomous agents: a header
//!             summary, a Table of safety toggles colored by state, and a
//!             footer carrying the read-model cursor plus key hints.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::AutonomyLensInput;
use jeryu_readmodel::HealthLevel;

pub fn draw(f: &mut Frame, input: &AutonomyLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / posture summary
            Constraint::Min(0),    // guardrail table
            Constraint::Length(3), // footer / cursor + keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_guardrails(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn yesno(value: bool) -> &'static str {
    if value { "yes" } else { "NO" }
}

/// Green when the gate is open (true), red+bold when it is shut (false).
fn gate_style(open: bool) -> Style {
    if open {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    }
}

fn health_style(level: HealthLevel) -> Style {
    match level {
        HealthLevel::Healthy => Style::default().fg(Color::Green),
        HealthLevel::Warning => Style::default().fg(Color::Yellow),
        HealthLevel::Degraded => Style::default().fg(Color::Yellow),
        HealthLevel::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        HealthLevel::Unknown => Style::default().fg(Color::DarkGray),
    }
}

fn draw_header(f: &mut Frame, input: &AutonomyLensInput, area: Rect) {
    let text = format!(
        "Autonomy — grants={} · code={}",
        input.active_grants,
        yesno(input.agents_can_code),
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Autonomy — Guardrails "),
        ),
        area,
    );
}

fn draw_guardrails(f: &mut Frame, input: &AutonomyLensInput, area: Rect) {
    // Blocked agents are an alert state: any blocked agent is a paused
    // guardrail, so colour the count red the moment it leaves zero.
    let blocked_style = if input.blocked_agents > 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    let rows = vec![
        Row::new(vec![
            Cell::from("Active grants"),
            Cell::from(input.active_grants.to_string()),
        ]),
        Row::new(vec![
            Cell::from("Agents can code"),
            Cell::from(Span::styled(
                yesno(input.agents_can_code).to_string(),
                gate_style(input.agents_can_code),
            )),
        ]),
        Row::new(vec![
            Cell::from("Safe to code"),
            Cell::from(Span::styled(
                yesno(input.safe_to_code).to_string(),
                gate_style(input.safe_to_code),
            )),
        ]),
        Row::new(vec![
            Cell::from("Blocked agents"),
            Cell::from(Span::styled(
                input.blocked_agents.to_string(),
                blocked_style,
            )),
        ]),
        Row::new(vec![
            Cell::from("Overall posture"),
            Cell::from(Span::styled(
                input.overall.label().to_string(),
                health_style(input.overall),
            )),
        ]),
    ];

    let table = Table::new(rows, [Constraint::Length(18), Constraint::Min(8)])
        .header(
            Row::new(vec![Cell::from("GUARDRAIL"), Cell::from("STATE")])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Safety Posture "),
        );

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &AutonomyLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: g grants · p pause · ? help",
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

    fn ink(input: &AutonomyLensInput, w: u16, h: u16) -> String {
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
        let input = AutonomyLensInput::from_read_model(&TuiReadModel::default());
        let ink = ink(&input, 80, 24);
        assert!(ink.contains("Autonomy"));
        assert!(ink.contains("Safety Posture"));
        assert!(ink.contains("Agents can code"));
        assert!(ink.contains("cursor="));
        assert!(ink.contains("help"));
    }

    #[test]
    fn renders_default_at_120x36() {
        let input = AutonomyLensInput::from_read_model(&TuiReadModel::default());
        // Just asserts no panic at the larger geometry.
        let _ = ink(&input, 120, 36);
    }

    #[test]
    fn renders_locked_down_posture() {
        let mut model = TuiReadModel::default();
        model.mission.active_grants = 4;
        model.mission.agents_can_code = false;
        model.mission.safe_to_code = false;
        model.mission.blocked_agents = 2;
        model.mission.overall = HealthLevel::Critical;
        model.event_cursor = 42;
        let input = AutonomyLensInput::from_read_model(&model);
        let ink = ink(&input, 120, 36);
        assert!(ink.contains("grants=4"));
        assert!(ink.contains("NO"));
        assert!(ink.contains("CRITICAL"));
        assert!(ink.contains("cursor=42"));
    }
}
