//! Owner: Interactive TUI subsystem - Git lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::git::view`
//! Invariants: Pure draw. Reads `GitLensInput`; no I/O. Renders the recent git
//!             command / sync ledger (one row per event) with a posture header
//!             and the selected row highlighted. Only renders the already
//!             redacted argv — never a raw command. Replaces the legacy
//!             git-sync panel (`draw_git_tab`).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::{GitEventRow, GitLensInput};

pub fn draw(f: &mut Frame, input: &GitLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // posture header
            Constraint::Min(0),    // event ledger
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

/// Color a per-event status word by its outcome (mirrors the legacy panel).
fn status_style(status: &str) -> Style {
    match status {
        "failed" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::Green),
    }
}

fn draw_header(f: &mut Frame, input: &GitLensInput, area: Rect) {
    let total = input.rows.len();
    let failed = input.failed_count();
    let posture = if total == 0 {
        "idle".to_string()
    } else if failed > 0 {
        format!("{failed} failed")
    } else {
        "all clean".to_string()
    };
    let text = format!("Git Sync — {total} recent operations · {posture}");
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Git — Command Ledger "),
        ),
        area,
    );
}

fn draw_body(f: &mut Frame, input: &GitLensInput, area: Rect) {
    if input.rows.is_empty() {
        f.render_widget(
            Paragraph::new("No recent git operations.")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title(" History ")),
            area,
        );
        return;
    }

    // Clamp the cursor so an out-of-range selection never panics or mis-highlights.
    let selected = input.selected.min(input.rows.len().saturating_sub(1));

    let header = Row::new(vec![
        Cell::from("TIME"),
        Cell::from("CLASS"),
        Cell::from("STATUS"),
        Cell::from("MIRROR"),
        Cell::from("COMMAND"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .rows
        .iter()
        .enumerate()
        .map(|(i, ev)| {
            let row = event_row(ev);
            if i == selected {
                row.style(
                    Style::default()
                        .bg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16), // TIME
            Constraint::Length(12), // CLASS
            Constraint::Length(8),  // STATUS
            Constraint::Length(8),  // MIRROR
            Constraint::Min(20),    // COMMAND
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" History "));

    f.render_widget(table, area);
}

fn event_row(ev: &GitEventRow) -> Row<'static> {
    // A 16-char timestamp prefix matches the legacy panel's `created_at[..16]`.
    let ts = ev
        .created_at
        .get(..16)
        .unwrap_or(&ev.created_at)
        .to_string();
    let status = ev.status();
    Row::new(vec![
        Cell::from(ts),
        Cell::from(ev.command_class.clone()),
        Cell::from(Span::styled(status.to_string(), status_style(status))),
        Cell::from(ev.mirror_status.clone()),
        // argv_redacted is already redacted — render verbatim.
        Cell::from(ev.argv_redacted.clone()),
    ])
}

fn draw_footer(f: &mut Frame, _input: &GitLensInput, area: Rect) {
    let text = " · Keys: ↑/↓ select · enter detail · e evidence";
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GitCommandEventRecord;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn record(class: &str, exit: i32, mirror: &str, argv: &str) -> GitCommandEventRecord {
        GitCommandEventRecord {
            id: 1,
            request_id: "req".into(),
            actor: "actor".into(),
            cwd: "/repo".into(),
            repo_root: Some("/repo".into()),
            argv_redacted: argv.into(),
            argv_hash: "hash".into(),
            command_class: class.into(),
            risk: "low".into(),
            mode: "exec".into(),
            before_head: None,
            before_branch: None,
            before_dirty: None,
            after_head: None,
            after_branch: None,
            after_dirty: None,
            exit_code: exit,
            sidecar_status: "ok".into(),
            mirror_status: mirror.into(),
            created_at: "2026-05-29T12:00:00Z".into(),
            payload: "{}".into(),
        }
    }

    fn ink(width: u16, height: u16, input: &GitLensInput) -> String {
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
    fn renders_empty_state_at_80x24_without_panic() {
        let input = GitLensInput::from_state(&[], 0);
        let s = ink(80, 24, &input);
        assert!(s.contains("Git"));
        assert!(s.contains("No recent git operations."));
    }

    #[test]
    fn renders_ledger_at_120x36_without_panic() {
        let events = vec![
            record("push", 0, "synced", "git push origin main"),
            record("fetch", 128, "n/a", "git fetch --all"),
        ];
        let input = GitLensInput::from_state(&events, 1);
        let s = ink(120, 36, &input);
        assert!(s.contains("Git"));
        assert!(s.contains("COMMAND"));
        assert!(s.contains("push"));
        assert!(s.contains("failed"));
    }

    #[test]
    fn out_of_range_selection_does_not_panic() {
        let events = vec![record("push", 0, "synced", "git push origin main")];
        let input = GitLensInput::from_state(&events, 99);
        // Both reference geometries must render cleanly with a bad cursor.
        let _ = ink(80, 24, &input);
        let _ = ink(120, 36, &input);
    }
}
