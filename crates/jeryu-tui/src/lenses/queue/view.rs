//! Queue lens view.
//!
//! Invariants: pure draw. Reads [`QueueLensInput`]; no backend I/O. Shows queue
//! depth against the fleet's theoretical capacity (runner headroom +
//! utilization) so the operator can see at a glance whether work will wait.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::QueueLensInput;

pub fn draw(f: &mut Frame, input: &QueueLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / queue posture
            Constraint::Min(0),    // capacity table
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_capacity_table(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &QueueLensInput, area: Rect) {
    let text = format!(
        "Queue — {} queued · {} running · {} failed · capacity {}/{}",
        input.queue_depth,
        input.running_jobs,
        input.failed_jobs,
        input.active_runners,
        input.total_runners,
    );
    let style = if input.queue_exceeds_headroom() {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    f.render_widget(
        Paragraph::new(Span::styled(text, style)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Queue — Depth & Capacity "),
        ),
        area,
    );
}

fn count_cell(value: u32, warn: bool) -> Cell<'static> {
    let style = if warn && value > 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    Cell::from(Span::styled(value.to_string(), style))
}

fn draw_capacity_table(f: &mut Frame, input: &QueueLensInput, area: Rect) {
    let headroom = input.headroom();
    let util = input.utilization_pct();

    let headroom_note = if input.queue_exceeds_headroom() {
        format!("{} free — queue exceeds free capacity", headroom)
    } else {
        format!("{} free", headroom)
    };
    let headroom_style = if input.queue_exceeds_headroom() {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    let rows = vec![
        Row::new(vec![
            Cell::from("Queued jobs"),
            count_cell(input.queue_depth, false),
            Cell::from("waiting for a free runner"),
        ]),
        Row::new(vec![
            Cell::from("Running jobs"),
            count_cell(input.running_jobs, false),
            Cell::from("currently executing"),
        ]),
        Row::new(vec![
            Cell::from("Failed jobs"),
            count_cell(input.failed_jobs, true),
            Cell::from("need attention"),
        ]),
        Row::new(vec![
            Cell::from("Runner capacity"),
            Cell::from(format!("{}/{}", input.active_runners, input.total_runners)),
            Cell::from(format!("{}% utilization", util)),
        ]),
        Row::new(vec![
            Cell::from("Headroom"),
            Cell::from(Span::styled(headroom.to_string(), headroom_style)),
            Cell::from(Span::styled(headroom_note, headroom_style)),
        ]),
    ];

    let header = Row::new(vec![
        Cell::from("METRIC"),
        Cell::from("VALUE"),
        Cell::from("NOTE"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Capacity "));

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &QueueLensInput, area: Rect) {
    let line = Line::from(format!(
        "cursor={} · Keys: s scale · d drain · ? help",
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
    use jeryu_readmodel::TuiReadModel;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(width: u16, height: u16, input: &QueueLensInput) -> String {
        let backend = TestBackend::new(width, height);
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
    fn renders_default_queue_at_80x24() {
        let input = QueueLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Queue"));
        assert!(out.contains("Capacity"));
        assert!(out.contains("Headroom"));
        assert!(out.contains("cursor="));
        assert!(out.contains("scale"));
    }

    #[test]
    fn renders_populated_queue_at_120x36() {
        let mut model = TuiReadModel {
            event_cursor: 99,
            ..Default::default()
        };
        model.mission.queued_jobs = 12;
        model.mission.running_jobs = 4;
        model.mission.failed_jobs = 2;
        model.mission.active_runners = 6;
        model.mission.total_runners = 8;
        let input = QueueLensInput::from_read_model(&model);
        let out = ink(120, 36, &input);
        assert!(out.contains("12 queued"));
        assert!(out.contains("utilization"));
        // queue (12) exceeds headroom (2) → warning copy rendered.
        assert!(out.contains("exceeds free capacity"));
        assert!(out.contains("cursor=99"));
    }

    #[test]
    fn renders_no_placeholder_markers() {
        let input = QueueLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(120, 36, &input);
        for marker in ["placeholder", "TODO"] {
            assert!(!out.contains(marker), "leftover marker leaked: {marker}");
        }
    }
}
