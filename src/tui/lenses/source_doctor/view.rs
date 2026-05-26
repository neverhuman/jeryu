//! Owner: Interactive TUI subsystem - Source Doctor lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::source_doctor::view`
//! Invariants: Pure draw. Reads `SourceDoctorLensInput`; never touches
//!             DB, GitLab, Docker, Vault, filesystem, MCP, or network
//!             during render. Per-source freshness, schema drift, action
//!             drift, MCP drift, docs drift, and DB profile mismatch
//!             land in U29 proper.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::SourceDoctorLensInput;

pub fn draw(f: &mut Frame, input: &SourceDoctorLensInput, area: Rect) {
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

fn draw_header(f: &mut Frame, input: &SourceDoctorLensInput, area: Rect) {
    let text = format!(
        "Source Doctor — {}/{} healthy  |  {} degraded  |  Cursor: {}",
        input.sources_healthy, input.sources_total, input.sources_degraded, input.event_cursor,
    );
    let p = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Source Doctor "),
    );
    f.render_widget(p, area);
}

fn draw_body_placeholder(f: &mut Frame, _input: &SourceDoctorLensInput, area: Rect) {
    let p = Paragraph::new(
        "(per-source freshness, schema drift, action drift, MCP drift, docs drift, DB profile mismatch — lands in U29)",
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Sources ")
            .border_style(Style::default().add_modifier(Modifier::DIM)),
    );
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, _input: &SourceDoctorLensInput, area: Rect) {
    let p = Paragraph::new(
        "Enter: drill source  |  Esc: back  |  e: errors  |  r: reconnect  |  ?: help",
    )
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
    fn renders_default_at_80x24() {
        let model = TuiReadModel::default();
        let input = SourceDoctorLensInput::from_read_model(&model);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(!ink.trim().is_empty());
        assert!(ink.contains("Source Doctor"));
        assert!(ink.contains("Sources"));
        assert!(ink.contains("Help"));
    }

    #[test]
    fn renders_default_at_120x36() {
        let model = TuiReadModel::default();
        let input = SourceDoctorLensInput::from_read_model(&model);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("healthy"));
    }
}
