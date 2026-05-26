//! Owner: Interactive TUI subsystem - Evidence lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::evidence::view`
//! Invariants: Pure draw. Reads `EvidenceLensInput`; never touches DB,
//!             GitLab, Docker, Vault, filesystem, MCP, or network during
//!             render. Real proof timeline, entity proof graph, capsule
//!             ledger, query, proof viewer, and bundle export land in
//!             U21 proper.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::EvidenceLensInput;

pub fn draw(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body_placeholder(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let text = format!(
        "Evidence ({} attention items)  |  Cursor: {}",
        input.attention_count, input.event_cursor,
    );
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Evidence "));
    f.render_widget(p, area);
}

fn draw_body_placeholder(f: &mut Frame, _input: &EvidenceLensInput, area: Rect) {
    let p = Paragraph::new(
        "(proof timeline + entity proof graph lands in U21 proper — capsule ledger, query, proof viewer, redacted bundle export)",
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Flight Recorder ")
            .border_style(Style::default().add_modifier(Modifier::DIM)),
    );
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, _input: &EvidenceLensInput, area: Rect) {
    let p =
        Paragraph::new("Enter: drill proof  |  Esc: back  |  /: filter  |  e: open  |  ?: help")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .border_style(Style::default().add_modifier(Modifier::BOLD)),
            );
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::TuiReadModel;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_default_evidence_at_80x24() {
        let model = TuiReadModel::default();
        let input = EvidenceLensInput::from_read_model(&model);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Evidence"));
        assert!(ink.contains("Flight Recorder"));
        assert!(ink.contains("Help"));
    }

    #[test]
    fn renders_default_evidence_at_120x36() {
        let model = TuiReadModel::default();
        let input = EvidenceLensInput::from_read_model(&model);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
    }
}
