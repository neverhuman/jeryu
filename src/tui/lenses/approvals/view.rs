//! Owner: Interactive TUI subsystem - Approvals lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::approvals::view`
//! Invariants: Pure draw. Reads `ApprovalsLensInput`; no I/O. Preserves the
//!             legacy two-pane approvals queue: a queue Table (PR#/RISK/CI/
//!             AGENT/AGE/TITLE, selected row highlighted, CI colored by status)
//!             beside a detail + actions inspector for the selected PR.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

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
    draw_footer(f, chunks[2]);
}

/// Truncate `text` to `max` chars, appending an ellipsis when clipped.
fn short_text(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Risk tier color: T0 gray → T4 red (ported from the legacy approvals tab).
fn tier_color(tier: u8) -> Color {
    match tier {
        0 => Color::Gray,
        1 => Color::Green,
        2 => Color::Cyan,
        3 => Color::Yellow,
        4 => Color::Red,
        _ => Color::Gray,
    }
}

/// CI status color: green pass / blue running / red fail / yellow otherwise.
fn ci_color(ci: &str) -> Color {
    match ci {
        "green" | "success" | "pass" => Color::Green,
        "running" | "pending" => Color::Blue,
        "failed" | "fail" | "error" => Color::Red,
        _ => Color::Yellow,
    }
}

fn draw_header(f: &mut Frame, input: &ApprovalsLensInput, area: Rect) {
    let text = format!("Approvals — {} pending", input.rows.len());
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
        Cell::from("AGENT"),
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
                    format!("T{}", p.risk_tier),
                    Style::default()
                        .fg(tier_color(p.risk_tier))
                        .add_modifier(Modifier::BOLD),
                )),
                Cell::from(Span::styled(
                    p.ci_status.clone(),
                    Style::default().fg(ci_color(&p.ci_status)),
                )),
                Cell::from(short_text(&p.agent_id, 12)),
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
            Constraint::Length(13),
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
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("PR #", Style::default().fg(Color::Gray)),
        Span::styled(
            p.pr_number.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(p.title.clone()));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("agent: ", Style::default().fg(Color::Gray)),
        Span::raw(p.agent_id.clone()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("tier:  ", Style::default().fg(Color::Gray)),
        Span::styled(
            format!("T{}", p.risk_tier),
            Style::default().fg(tier_color(p.risk_tier)),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("ci:    ", Style::default().fg(Color::Gray)),
        Span::styled(
            p.ci_status.clone(),
            Style::default().fg(ci_color(&p.ci_status)),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("age:   ", Style::default().fg(Color::Gray)),
        Span::raw(p.age.clone()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("sha:   ", Style::default().fg(Color::Gray)),
        Span::raw(short_text(&p.head_sha, 12)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Actions:",
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::UNDERLINED),
    )));
    lines.push(Line::from(vec![
        Span::styled("  a approve         ", Style::default().fg(Color::Green)),
        Span::styled(
            "jeryu release approve --pr N",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  r request changes ", Style::default().fg(Color::Yellow)),
        Span::styled(
            "post a structured review",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  e evidence        ", Style::default().fg(Color::Blue)),
        Span::styled(
            "open the evidence capsule",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    lines
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let text = " · Keys: ↑/↓ select · a approve · r reject · e evidence";
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::PendingApproval;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(width: u16, height: u16, input: &ApprovalsLensInput) -> String {
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

    fn pending(pr: u64, tier: u8, ci: &str, title: &str) -> PendingApproval {
        PendingApproval {
            pr_number: pr,
            title: title.into(),
            agent_id: format!("agent-{pr}"),
            risk_tier: tier,
            ci_status: ci.into(),
            age: "5m".into(),
            head_sha: "0badc0ffee1234".into(),
        }
    }

    #[test]
    fn short_text_truncates_with_ellipsis() {
        assert_eq!(short_text("short", 12), "short");
        assert_eq!(short_text("a really long title here", 8), "a reall…");
    }

    #[test]
    fn empty_queue_renders_empty_state_at_80x24() {
        let input = ApprovalsLensInput::from_state(&[], 0);
        let s = ink(80, 24, &input);
        assert!(s.contains("Approvals"));
        assert!(s.contains("No pending approvals"));
    }

    #[test]
    fn populated_queue_renders_rows_at_120x36() {
        let queue = vec![
            pending(101, 1, "green", "fix flaky integration test"),
            pending(102, 4, "failed", "risky schema migration"),
        ];
        let input = ApprovalsLensInput::from_state(&queue, 1);
        let s = ink(120, 36, &input);
        assert!(s.contains("Approvals"));
        // Header columns and a row word both render.
        assert!(s.contains("RISK"));
        assert!(s.contains("agent-102"));
        assert!(s.contains("Actions"));
    }

    #[test]
    fn renders_at_80x24_populated_without_panic() {
        let queue = vec![pending(7, 2, "running", "small change")];
        let input = ApprovalsLensInput::from_state(&queue, 0);
        let s = ink(80, 24, &input);
        assert!(s.contains("Approvals"));
        assert!(s.contains("#7"));
    }
}
