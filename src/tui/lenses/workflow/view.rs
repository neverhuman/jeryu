//! Owner: Interactive TUI subsystem - Workflow lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::workflow::view`
//! Invariants: Pure draw. Reads `WorkflowLensInput`; never touches DB,
//!             GitLab, Docker, Vault, filesystem, MCP, or network during
//!             render. Real DAG canvas + critical path + PR/phase rail
//!             + inspector + logs land in U19 (model + delivery) and
//!             U20 (canvas + rails + inspector + logs).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::WorkflowLensInput;

pub fn draw(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_canvas_placeholder(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let text = format!(
        "Workflow atlas — {} pipelines  |  {} blocked  |  Cursor: {}",
        input.total_pipelines, input.blocked_count, input.event_cursor,
    );
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Workflow "));
    f.render_widget(p, area);
}

fn draw_canvas_placeholder(f: &mut Frame, _input: &WorkflowLensInput, area: Rect) {
    let p = Paragraph::new(
        "(DAG canvas + critical path + PR/phase rail land in U20)",
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Atlas ")
            .border_style(Style::default().add_modifier(Modifier::DIM)),
    );
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, _input: &WorkflowLensInput, area: Rect) {
    let p = Paragraph::new(
        "Enter: drill pipeline  |  Esc: back  |  a: actions  |  e: evidence  |  l: logs  |  n: next  |  ?: help",
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
        let input = WorkflowLensInput::from_read_model(&model);
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
        assert!(ink.contains("Workflow"));
        assert!(ink.contains("Atlas"));
        assert!(ink.contains("Help"));
    }

    #[test]
    fn renders_default_at_120x36() {
        let model = TuiReadModel::default();
        let input = WorkflowLensInput::from_read_model(&model);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("pipelines"));
    }
}
