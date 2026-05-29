//! Owner: Interactive TUI subsystem - Queue lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::queue::view`
//! Invariants: Pure draw. Reads `QueueLensInput`; never touches DB, GitLab,
//!             Docker, Vault, filesystem, MCP, or network during render.
//!             Three-tier physics-floor capacity widget lands in U17 proper.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::QueueLensInput;

pub fn draw(f: &mut Frame, input: &QueueLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    draw_posture(f, input, chunks[0]);
    draw_capacity(f, input, chunks[1]);
    draw_placeholder(f, input, chunks[2]);
}

fn draw_posture(f: &mut Frame, input: &QueueLensInput, area: Rect) {
    let text = format!(
        "Queue: {} waiting / {} running / {} failed  |  Cursor: {}",
        input.queue_depth, input.running_jobs, input.failed_jobs, input.event_cursor,
    );
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Queue "));
    f.render_widget(p, area);
}

fn draw_capacity(f: &mut Frame, input: &QueueLensInput, area: Rect) {
    let scaling_hint = if input.queue_depth == 0 {
        "no queued jobs"
    } else if input.degraded_runners > 0 {
        "repair runner sources before adding runners"
    } else if input.active_runners < input.total_runners {
        "adding runners can help if queued jobs fit this pool"
    } else {
        "pool limit reached; adding managers alone will not help"
    };
    let text = format!(
        "Runners: {} active / {} total / {} degraded | {}",
        input.active_runners, input.total_runners, input.degraded_runners, scaling_hint,
    );
    let p = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Capacity ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(p, area);
}

fn draw_placeholder(f: &mut Frame, _input: &QueueLensInput, area: Rect) {
    let p = Paragraph::new("(physics floor calculation lands in U17 proper)").block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Theoretical Limit Lab ")
            .border_style(Style::default().add_modifier(Modifier::DIM)),
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
    fn renders_default_queue_at_80x24() {
        let model = TuiReadModel::default();
        let input = QueueLensInput::from_read_model(&model);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Queue"));
        assert!(ink.contains("Capacity"));
        assert!(ink.contains("Theoretical Limit Lab"));
    }

    #[test]
    fn renders_default_queue_at_120x36() {
        let model = TuiReadModel::default();
        let input = QueueLensInput::from_read_model(&model);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
    }
}
