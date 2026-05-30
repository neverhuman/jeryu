//! Approvals lens view.
//!
//! Invariants: pure draw. Reads [`ApprovalsLensInput`]; no backend I/O. Renders
//! the two-pane approvals queue: a queue table (PR#/RISK/CI/AUTHOR/AGE/TITLE,
//! selected row highlighted, checks colored by status) beside a PR inspector.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::{CheckStatus, RiskTier};

use super::data::{ApprovalRow, ApprovalsLensInput};

pub fn draw(f: &mut Frame, input: &ApprovalsLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / posture
            Constraint::Min(0),    // queue + inspector
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn short_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

fn risk_color(risk: RiskTier) -> Color {
    match risk {
        RiskTier::R0 => Color::Gray,
        RiskTier::R1 => Color::Green,
        RiskTier::R2 => Color::Cyan,
        RiskTier::R3 => Color::Yellow,
        RiskTier::R4 | RiskTier::R5 => Color::Red,
    }
}

fn checks_color(checks: CheckStatus) -> Color {
    match checks {
        CheckStatus::Success => Color::Green,
        CheckStatus::Pending => Color::Blue,
        CheckStatus::Failure => Color::Red,
        CheckStatus::Neutral => Color::Yellow,
    }
}

fn risk_tag(risk: RiskTier) -> String {
    match risk {
        RiskTier::R0 => "R0",
        RiskTier::R1 => "R1",
        RiskTier::R2 => "R2",
        RiskTier::R3 => "R3",
        RiskTier::R4 => "R4",
        RiskTier::R5 => "R5",
    }
    .to_string()
}

fn draw_header(f: &mut Frame, input: &ApprovalsLensInput, area: Rect) {
    let failing = input.failing_checks();
    let text = if failing > 0 {
        format!(
            "Approvals — {} pending · {} with failing checks",
            input.rows.len(),
            failing
        )
    } else {
        format!("Approvals — {} pending", input.rows.len())
    };
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Approvals — Awaiting human "),
        ),
        area,
    );
}

fn draw_body(f: &mut Frame, input: &ApprovalsLensInput, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    draw_queue(f, input, cols[0]);
    draw_inspector(f, input, cols[1]);
}

fn draw_queue(f: &mut Frame, input: &ApprovalsLensInput, area: Rect) {
    if input.is_empty() {
        f.render_widget(
            Paragraph::new("No pending approvals.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Awaiting approval (0) "),
            ),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("PR#"),
        Cell::from("RISK"),
        Cell::from("CI"),
        Cell::from("AUTHOR"),
        Cell::from("AGE"),
        Cell::from("TITLE"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .rows
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let row = Row::new(vec![
                Cell::from(Span::styled(
                    format!("#{}", p.pr_number),
                    Style::default().fg(Color::Cyan),
                )),
                Cell::from(Span::styled(
                    risk_tag(p.risk),
                    Style::default()
                        .fg(risk_color(p.risk))
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    p.checks.label().to_string(),
                    Style::default().fg(checks_color(p.checks)),
                )),
                Cell::from(short_text(&p.author, 14)),
                Cell::from(p.age.clone()),
                Cell::from(short_text(&p.title, 40)),
            ]);
            if i == input.selected {
                row.style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::REVERSED | Modifier::BOLD),
                )
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Length(9),
            Constraint::Length(15),
            Constraint::Length(6),
            Constraint::Min(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Awaiting approval ({}) ", input.rows.len())),
    );

    f.render_widget(table, area);
}

fn draw_inspector(f: &mut Frame, input: &ApprovalsLensInput, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(" Inspector ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = match input.selected_row() {
        Some(p) => detail_lines(p),
        None => vec![Line::from(Span::styled(
            "No pending approvals.",
            Style::default().fg(Color::DarkGray),
        ))],
    };

    f.render_widget(Paragraph::new(lines), inner);
}

fn detail_lines(p: &ApprovalRow) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("PR #", Style::default().fg(Color::Gray)),
            Span::styled(
                p.pr_number.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(p.title.clone()),
        Line::from(""),
        Line::from(vec![
            Span::styled("author: ", Style::default().fg(Color::Gray)),
            Span::raw(p.author.clone()),
        ]),
        Line::from(vec![
            Span::styled("risk:   ", Style::default().fg(Color::Gray)),
            Span::styled(p.risk.label(), Style::default().fg(risk_color(p.risk))),
        ]),
        Line::from(vec![
            Span::styled("checks: ", Style::default().fg(Color::Gray)),
            Span::styled(
                p.checks.label().to_string(),
                Style::default().fg(checks_color(p.checks)),
            ),
        ]),
        Line::from(vec![
            Span::styled("age:    ", Style::default().fg(Color::Gray)),
            Span::raw(p.age.clone()),
        ]),
        Line::from(vec![
            Span::styled("sha:    ", Style::default().fg(Color::Gray)),
            Span::raw(short_text(&p.head_sha, 12)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Actions:",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::UNDERLINED),
        )),
        Line::from(Span::styled(
            "  a approve · r request changes · e evidence",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

fn draw_footer(f: &mut Frame, input: &ApprovalsLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: ↑/↓ select · a approve · r reject · e evidence",
        input.event_cursor
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{TuiReadModel, sample_read_model};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, input: &ApprovalsLensInput) -> String {
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

    #[test]
    fn renders_empty_queue_at_80x24() {
        let input = ApprovalsLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Approvals"));
        assert!(out.contains("No pending approvals"));
    }

    #[test]
    fn renders_pending_prs_at_120x36() {
        let input = ApprovalsLensInput::from_read_model_selecting(&sample_read_model(), 1);
        let out = ink(120, 36, &input);
        assert!(out.contains("Approvals"));
        assert!(out.contains("RISK"));
        assert!(out.contains("#101"));
        assert!(out.contains("#102"));
        assert!(out.contains("agent-storm-04"));
        assert!(out.contains("failure"));
        assert!(out.contains("failing checks"));
        assert!(out.contains("Actions"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = ApprovalsLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
