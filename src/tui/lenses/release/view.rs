//! Owner: Interactive TUI subsystem - Release lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::release::view`
//! Invariants: Pure draw. Reads ReleaseLensInput; no I/O.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::ReleaseLensInput;

pub fn draw(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)])
        .split(area);

    let header = format!("Release — safe_to_release: {}", input.safe_to_release);
    f.render_widget(
        Paragraph::new(header).block(Block::default().borders(Borders::ALL).title(" Release ")),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new("(candidate/canary/production/gates/rollback land in U27)")
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
        let input = ReleaseLensInput::from_read_model(&TuiReadModel::default());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Release"));
    }

    #[test]
    fn renders_at_120x36() {
        let input = ReleaseLensInput::from_read_model(&TuiReadModel::default());
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
    }
}
