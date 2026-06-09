//! Evidence lens view.
//!
//! Invariants: pure draw. Reads [`EvidenceLensInput`]; no backend I/O. Renders
//! the proof ledger, codegraph/oracle evidence, tool-building opportunities,
//! and a footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::GateDecision;

use super::data::{CodegraphEvidenceRow, EvidenceLensInput, EvidenceRow, ToolBuildOpportunityRow};

pub fn draw(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / capsule summary
            Constraint::Min(0),    // proof ledger
            Constraint::Length(7), // codegraph/oracle evidence
            Constraint::Length(7), // tool-building opportunities
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_ledger(f, input, chunks[1]);
    draw_codegraph(f, input, chunks[2]);
    draw_tool_build(f, input, chunks[3]);
    draw_footer(f, input, chunks[4]);
}

fn draw_header(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let text = format!(
        "Evidence — {} capsules · {} open · {} denied · codegraph v{} · {} miss",
        input.total_capsules,
        input.open_capsules,
        input.denied(),
        input
            .codegraph_schema_version
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".into()),
        input.codegraph_misses,
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Evidence ")),
        area,
    );
}

fn decision_style(decision: GateDecision) -> Style {
    match decision {
        GateDecision::Allow => Style::default().fg(Color::Green),
        GateDecision::Deny => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        GateDecision::Pending => Style::default().fg(Color::Yellow),
        GateDecision::Recorded => Style::default().fg(Color::Gray),
    }
}

fn draw_ledger(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    if input.rows.is_empty() {
        f.render_widget(
            Paragraph::new("No proof receipts recorded.")
                .block(Block::default().borders(Borders::ALL).title(" Ledger ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("CAPSULE"),
        Cell::from("ENTITY"),
        Cell::from("DECISION"),
        Cell::from("RECEIPT"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input.rows.iter().map(receipt_row).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(20),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Ledger "));

    f.render_widget(table, area);
}

fn draw_codegraph(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    if input.codegraph_rows.is_empty() {
        f.render_widget(
            Paragraph::new("No codegraph oracle evidence.")
                .block(Block::default().borders(Borders::ALL).title(" Codegraph ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("QUERY"),
        Cell::from("TOOL"),
        Cell::from("SYMBOL"),
        Cell::from("REFS"),
        Cell::from("LANES"),
        Cell::from("READS"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = input.codegraph_rows.iter().map(codegraph_row).collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(16),
            Constraint::Length(18),
            Constraint::Length(6),
            Constraint::Length(22),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Codegraph Oracle "),
    );
    f.render_widget(table, area);
}

fn codegraph_row(r: &CodegraphEvidenceRow) -> Row<'_> {
    let reads = if r.required_reads.is_empty() {
        "—".into()
    } else {
        r.required_reads.join(",")
    };
    let lanes = if r.proof_lanes.is_empty() {
        "—".into()
    } else {
        r.proof_lanes.join(",")
    };
    let symbol = if let Some(miss) = &r.miss {
        format!("{} ({miss})", r.symbol)
    } else {
        r.symbol.clone()
    };
    Row::new(vec![
        Cell::from(r.query_id.clone()),
        Cell::from(r.tool.clone()),
        Cell::from(symbol),
        Cell::from(r.references.to_string()),
        Cell::from(lanes),
        Cell::from(reads),
    ])
}

fn draw_tool_build(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    if input.tool_build_rows.is_empty() {
        f.render_widget(
            Paragraph::new("No tool-building opportunities.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Tool-building opportunities "),
            ),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("CLUSTER"),
        Cell::from("REPO"),
        Cell::from("SCORE"),
        Cell::from("OCC"),
        Cell::from("FILES"),
        Cell::from("LANG"),
        Cell::from("PROOF LANE"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = input.tool_build_rows.iter().map(tool_build_row).collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(22),
            Constraint::Length(16),
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Min(24),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Tool-building opportunities "),
    );
    f.render_widget(table, area);
}

fn tool_build_row(r: &ToolBuildOpportunityRow) -> Row<'_> {
    Row::new(vec![
        Cell::from(r.cluster_id.clone()),
        Cell::from(r.repo_id.clone()),
        Cell::from(r.score.to_string()),
        Cell::from(r.occurrences.to_string()),
        Cell::from(r.file_count.to_string()),
        Cell::from(r.language.clone()),
        Cell::from(r.suggested_proof_lane.clone()),
    ])
}

fn receipt_row(r: &EvidenceRow) -> Row<'_> {
    let label = if r.redacted {
        format!("{} (redacted)", r.label)
    } else {
        r.label.clone()
    };
    Row::new(vec![
        Cell::from(r.capsule_id.clone()),
        Cell::from(r.entity.display()),
        Cell::from(Span::styled(
            r.decision.label().to_string(),
            decision_style(r.decision),
        )),
        Cell::from(label),
    ])
}

fn draw_footer(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let line = Line::from(format!(
        "cursor={} · Keys: / search · enter open · y copy",
        input.event_cursor
    ));
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL).title(" Keys ")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{TuiReadModel, sample_read_model};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, input: &EvidenceLensInput) -> String {
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
    fn renders_empty_at_80x24() {
        let input = EvidenceLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Evidence"));
        assert!(out.contains("capsules"));
        assert!(out.contains("No proof receipts recorded."));
        assert!(out.contains("No tool-building opportunities."));
        assert!(out.contains("cursor="));
    }

    #[test]
    fn renders_receipts_at_120x36() {
        let input = EvidenceLensInput::from_read_model(&sample_read_model());
        let out = ink(120, 36, &input);
        assert!(out.contains("17 capsules"));
        assert!(out.contains("cap-17"));
        assert!(out.contains("DECISION"));
        assert!(out.contains("allow"));
        assert!(out.contains("deny"));
        assert!(out.contains("redacted"));
        assert!(out.contains("1 denied"));
        assert!(out.contains("codegraph v2"));
        assert!(out.contains("codegraph.query"));
        assert!(out.contains("AgentRunStore"));
        assert!(out.contains("codegraph-oracle"));
        assert!(out.contains("Tool-building opportunities"));
        assert!(out.contains("toolbuild-agent-runner"));
        assert!(out.contains("core/api"));
        assert!(out.contains("rust"));
        assert!(out.contains("codegraph-tool-build"));
        assert!(!out.contains("not yet ported"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = EvidenceLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
