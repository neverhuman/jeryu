//! Owner: Interactive TUI subsystem - Evidence lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::evidence::view`
//! Invariants: Pure draw. Reads `EvidenceLensInput`; never touches DB,
//!             GitLab, Docker, Vault, filesystem, MCP, or network during
//!             render. Renders the proof ledger: a capsule-count header, a
//!             table of evidence references (source → ref), and a key hint
//!             footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::EvidenceLensInput;

pub fn draw(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / capsule summary
            Constraint::Min(0),    // evidence ledger
            Constraint::Length(3), // footer / key hints
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_ledger(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let text = format!(
        "Evidence — {} capsules · {} open",
        input.evidence_count, input.open_capsules,
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Evidence ")),
        area,
    );
}

fn draw_ledger(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    if input.rows.is_empty() {
        f.render_widget(
            Paragraph::new("No evidence capsules recorded.")
                .block(Block::default().borders(Borders::ALL).title(" Ledger ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![Cell::from("SOURCE"), Cell::from("EVIDENCE REF")])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .rows
        .iter()
        .map(|r| {
            Row::new(vec![
                Cell::from(r.source_title.clone()),
                Cell::from(r.evidence_ref.clone()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Percentage(40), Constraint::Percentage(60)],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Ledger "));

    f.render_widget(table, area);
}

fn draw_footer(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: / search · enter open · y copy",
        input.event_cursor,
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .border_style(Style::default().add_modifier(Modifier::BOLD)),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef, Severity};
    use crate::api::read_model::{AttentionItem, TuiReadModel};
    use chrono::Utc;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn model_with_evidence() -> TuiReadModel {
        let now = Utc::now();
        let mut model = TuiReadModel::default();
        model.mission.evidence_count = 5;
        model.mission.open_capsules = 2;
        model.attention.push(AttentionItem {
            id: "a0".into(),
            severity: Severity::Warning,
            title: "merge gate failed".into(),
            why_it_matters: "blocks release".into(),
            entity: EntityRef::new(EntityKind::Project, "a0"),
            evidence: vec!["cap://abc123".into()],
            recommended_actions: Vec::new(),
            created_at: now,
            last_seen_at: now,
        });
        model
    }

    #[test]
    fn renders_default_evidence_at_80x24() {
        let model = TuiReadModel::default();
        let input = EvidenceLensInput::from_read_model(&model);
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("Evidence"));
        assert!(ink.contains("capsules"));
        assert!(ink.contains("No evidence capsules recorded."));
        assert!(ink.contains("cursor="));
    }

    #[test]
    fn renders_ledger_rows_at_120x36() {
        let model = model_with_evidence();
        let input = EvidenceLensInput::from_read_model(&model);
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(f, &input, f.area());
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let ink: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(ink.contains("merge gate failed"));
        assert!(ink.contains("cap://abc123"));
        assert!(ink.contains("EVIDENCE REF"));
    }
}
