//! Owner: Interactive TUI subsystem - Repos lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::repos::view`
//! Invariants: Pure draw. Reads `ReposLensInput`; never touches DB, GitLab,
//!             Docker, Vault, filesystem, MCP, or network during render.
//!             Real family/repo trees, scoped attention, and freshness
//!             badges land in U18 proper.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::ReposLensInput;

pub fn draw(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_list_placeholder(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let text = format!(
        "Repos & Families ({} repos)  |  Cursor: {}",
        input.total_repos, input.event_cursor,
    );
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Repos "));
    f.render_widget(p, area);
}

fn draw_list_placeholder(f: &mut Frame, _input: &ReposLensInput, area: Rect) {
    let p = Paragraph::new(
        "(family/repo enumeration lands in U18 proper — fleet -> family -> repo drilldown)",
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" List ")
            .border_style(Style::default().add_modifier(Modifier::DIM)),
    );
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, _input: &ReposLensInput, area: Rect) {
    let p = Paragraph::new("Enter: drill repo  |  Esc: back  |  e: evidence  |  ?: help").block(
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
    fn renders_default_repos_at_80x24() {
        let model = TuiReadModel::default();
        let input = ReposLensInput::from_read_model(&model);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Repos"));
        assert!(ink.contains("List"));
        assert!(ink.contains("Help"));
    }

    #[test]
    fn renders_default_repos_at_120x36() {
        let model = TuiReadModel::default();
        let input = ReposLensInput::from_read_model(&model);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
    }
}
