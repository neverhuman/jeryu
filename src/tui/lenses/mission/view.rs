//! Owner: Interactive TUI subsystem - Mission lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::mission::view`
//! Invariants: Pure draw. Reads `MissionLensInput`; never touches DB, GitLab,
//!             Docker, Vault, filesystem, MCP, or network during render.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::MissionLensInput;

pub fn draw(f: &mut Frame, input: &MissionLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_posture(f, input, chunks[0]);
    draw_attention(f, input, chunks[1]);
    draw_next_action(f, input, chunks[2]);
}

fn draw_posture(f: &mut Frame, input: &MissionLensInput, area: Rect) {
    let text = format!(
        "Overall: {:?}  |  Safe to code: {}  Safe to merge: {}  Safe to release: {}  |  Cursor: {}",
        input.mission.overall,
        if input.mission.safe_to_code {
            "yes"
        } else {
            "NO"
        },
        if input.mission.safe_to_merge {
            "yes"
        } else {
            "NO"
        },
        if input.mission.safe_to_release {
            "yes"
        } else {
            "NO"
        },
        input.event_cursor,
    );
    let p = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Mission Control — Posture "),
    );
    f.render_widget(p, area);
}

fn draw_attention(f: &mut Frame, input: &MissionLensInput, area: Rect) {
    let lines: Vec<Line> = if input.attention.is_empty() {
        vec![Line::from("(no attention items)")]
    } else {
        input
            .attention
            .iter()
            .take(20)
            .map(|item| {
                Line::from(format!(
                    "[{:?}] {} — {}",
                    item.severity, item.title, item.why_it_matters
                ))
            })
            .collect()
    };
    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Attention ")
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(p, area);
}

fn draw_next_action(f: &mut Frame, input: &MissionLensInput, area: Rect) {
    let text = match &input.next_action {
        Some(na) => format!("Next: {} — {} (risk={:?})", na.label, na.why, na.risk),
        None => "(no recommendation)".to_string(),
    };
    let p = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Next Action ")
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
    fn renders_default_mission_at_80x24() {
        let model = TuiReadModel::default();
        let input = MissionLensInput::from_read_model(&model);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Posture"));
        assert!(ink.contains("Attention"));
        assert!(ink.contains("Next Action"));
    }

    #[test]
    fn renders_default_mission_at_120x36() {
        let model = TuiReadModel::default();
        let input = MissionLensInput::from_read_model(&model);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
    }
}
