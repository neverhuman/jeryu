//! Owner: Interactive TUI subsystem - LLMs lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::llms::view`
//! Invariants: Pure draw. Reads LlmsLensInput; no I/O.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::LlmsLensInput;

pub fn draw(f: &mut Frame, input: &LlmsLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    let header = "LLMs — provider/budget land in U25";
    f.render_widget(
        Paragraph::new(header).block(Block::default().borders(Borders::ALL).title(" LLMs ")),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new("(calls/budget/traces/providers/keys land in U25)")
            .block(Block::default().borders(Borders::ALL)),
        chunks[1],
    );
    f.render_widget(
        Paragraph::new(format!("cursor={}", input.event_cursor))
            .block(Block::default().borders(Borders::ALL)),
        chunks[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_at_80x24() {
        let input = LlmsLensInput::from_read_model(&TuiReadModel::default());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("LLMs"));
    }

    #[test]
    fn renders_at_120x36() {
        let input = LlmsLensInput::from_read_model(&TuiReadModel::default());
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
    }
}
