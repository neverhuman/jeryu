//! Owner: Interactive TUI subsystem - LLMs lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::llms::view`
//! Invariants: Pure draw. Reads `LlmsLensInput`; no I/O. Renders the LLM/agent
//!             model-access ledger (active sessions, grants, access posture)
//!             from the mission proxy. A genuine empty state appears when no
//!             agents hold model access.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::LlmsLensInput;

pub fn draw(f: &mut Frame, input: &LlmsLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // ledger table
            Constraint::Length(3), // footer
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_ledger(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &LlmsLensInput, area: Rect) {
    let text = format!(
        "LLMs — {} agent sessions · {} grants",
        input.active_agents, input.active_grants
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" LLMs ")),
        area,
    );
}

fn posture_cell(can_code: bool) -> Cell<'static> {
    if can_code {
        Cell::from(Span::styled(
            "yes",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Cell::from(Span::styled(
            "NO",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    }
}

fn draw_ledger(f: &mut Frame, input: &LlmsLensInput, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Model Access ");

    // Genuine empty state: no agents currently hold LLM/model access.
    if input.active_agents == 0 {
        f.render_widget(Paragraph::new("No active LLM sessions.").block(block), area);
        return;
    }

    let header = Row::new(vec![Cell::from("METRIC"), Cell::from("VALUE")])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows = vec![
        Row::new(vec![
            Cell::from("Active agents (LLM consumers)"),
            Cell::from(input.active_agents.to_string()),
        ]),
        Row::new(vec![
            Cell::from("Active grants"),
            Cell::from(input.active_grants.to_string()),
        ]),
        Row::new(vec![
            Cell::from("Access posture (can code)"),
            posture_cell(input.agents_can_code),
        ]),
    ];

    let table = Table::new(rows, [Constraint::Min(32), Constraint::Length(12)])
        .header(header)
        .block(block);

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &LlmsLensInput, area: Rect) {
    let line = Line::from(format!(
        "cursor={} · Keys: enter detail · a actions",
        input.event_cursor
    ));
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::{MissionSnapshot, TuiReadModel};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(input: &LlmsLensInput, w: u16, h: u16) -> String {
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

    fn input_with_sessions() -> LlmsLensInput {
        LlmsLensInput::from_read_model(&TuiReadModel {
            mission: MissionSnapshot {
                active_agents: 4,
                active_grants: 2,
                agents_can_code: false,
                ..Default::default()
            },
            event_cursor: 42,
            ..Default::default()
        })
    }

    #[test]
    fn renders_empty_state_at_80x24() {
        let input = LlmsLensInput::from_read_model(&TuiReadModel::default());
        let ink = ink(&input, 80, 24);
        assert!(ink.contains("LLMs"), "header must render");
        assert!(
            ink.contains("No active LLM sessions."),
            "empty state must render"
        );
        assert!(ink.contains("cursor=0"), "footer cursor must render");
    }

    #[test]
    fn renders_ledger_with_posture_at_120x36() {
        let input = input_with_sessions();
        let ink = ink(&input, 120, 36);
        assert!(ink.contains("Active agents"), "agents row must render");
        assert!(ink.contains("Active grants"), "grants row must render");
        assert!(ink.contains("Access posture"), "posture row must render");
        assert!(ink.contains("NO"), "blocked posture must render");
        assert!(ink.contains("cursor=42"), "footer cursor must render");
    }

    #[test]
    fn renders_at_80x24_with_sessions() {
        // Smaller terminal must not panic with the table populated.
        let input = input_with_sessions();
        let _ = ink(&input, 80, 24);
    }
}
