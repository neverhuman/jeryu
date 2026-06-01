//! Owner: Interactive TUI subsystem - Bugs lens view
//! Proof: `cargo test -p jeryu-tui --lib tui::lenses::bugs::view`
//! Invariants: Pure draw. Reads `BugsLensInput`; never touches DB, GitLab,
//!             Docker, Vault, filesystem, MCP, or network during render.
//!             Renders the bug/blocker triage queue: a header summarizing the
//!             top blocker and failing-job pressure, a severity-colored table
//!             of attention items, and a keys footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::{BugRow, BugsLensInput};
use jeryu_readmodel::Severity;

pub fn draw(f: &mut Frame, input: &BugsLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / top blocker + failing jobs
            Constraint::Min(0),    // triage table
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_triage(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn severity_style(sev: Severity) -> Style {
    match sev {
        Severity::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        Severity::Error => Style::default().fg(Color::LightRed),
        Severity::Warning => Style::default().fg(Color::Yellow),
        Severity::Info => Style::default().fg(Color::Gray),
    }
}

fn draw_header(f: &mut Frame, input: &BugsLensInput, area: Rect) {
    let blocker = input.top_blocker.as_deref().unwrap_or("none");
    let line = Line::from(vec![
        Span::raw("Bugs — top blocker: "),
        Span::styled(
            blocker.to_string(),
            match input.top_blocker_severity {
                Some(sev) => severity_style(sev),
                None => Style::default().fg(Color::Green),
            },
        ),
        Span::raw(format!(" · {} failing jobs", input.failed_jobs)),
    ]);
    f.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Bugs — Triage "),
        ),
        area,
    );
}

fn draw_triage(f: &mut Frame, input: &BugsLensInput, area: Rect) {
    if input.rows.is_empty() {
        f.render_widget(
            Paragraph::new("No open bugs or blockers.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Open Bugs & Blockers "),
            ),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("SEV"),
        Cell::from("BUG"),
        Cell::from("WHY IT MATTERS"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .rows
        .iter()
        .map(|r: &BugRow| {
            Row::new(vec![
                Cell::from(Span::styled(
                    r.severity.label().to_string(),
                    severity_style(r.severity),
                )),
                Cell::from(r.title.clone()),
                Cell::from(r.why_it_matters.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(40),
            Constraint::Percentage(55),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Open Bugs & Blockers ")
            .border_style(Style::default().fg(Color::Yellow)),
    );

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &BugsLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: enter open · a actions · e evidence",
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
    use chrono::Utc;
    use jeryu_readmodel::{AttentionItem, MissionSnapshot, TuiReadModel};
    use jeryu_readmodel::{BlockerSummary, EntityKind, EntityRef};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn attention(sev: Severity, title: &str, why: &str) -> AttentionItem {
        AttentionItem {
            id: title.into(),
            severity: sev,
            title: title.into(),
            why_it_matters: why.into(),
            entity: EntityRef::new(EntityKind::Bug, title),
            evidence: Vec::new(),
            recommended_actions: Vec::new(),
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
        }
    }

    fn populated_input() -> BugsLensInput {
        let model = TuiReadModel {
            event_cursor: 7,
            mission: MissionSnapshot {
                failed_jobs: 2,
                top_blocker: Some(BlockerSummary {
                    kind: "merge_gate".into(),
                    severity: Severity::Critical,
                    summary: "release gate failing".into(),
                    entity: None,
                    recommended_action: None,
                }),
                ..Default::default()
            },
            attention: vec![
                attention(Severity::Error, "flaky-test", "blocks merge of MR 42"),
                attention(Severity::Warning, "stale-capsule", "evidence outdated"),
            ],
            ..Default::default()
        };
        BugsLensInput::from_read_model(&model)
    }

    #[test]
    fn renders_at_80x24() {
        let input = BugsLensInput::from_read_model(&TuiReadModel::default());
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Bugs"));
        assert!(ink.contains("No open bugs or blockers"));
        assert!(ink.contains("cursor=0"));
    }

    #[test]
    fn renders_at_120x36() {
        let input = BugsLensInput::from_read_model(&TuiReadModel::default());
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
    }

    #[test]
    fn renders_triage_rows_and_blocker() {
        let input = populated_input();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &input, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("top blocker"));
        assert!(ink.contains("release gate failing"));
        assert!(ink.contains("2 failing jobs"));
        assert!(ink.contains("flaky-test"));
        assert!(ink.contains("stale-capsule"));
        assert!(ink.contains("cursor=7"));
    }
}
