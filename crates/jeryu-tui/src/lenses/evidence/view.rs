//! Evidence lens view.
//!
//! Invariants: pure draw. Reads [`EvidenceLensInput`]; no backend I/O. Renders
//! the proof ledger: a capsule-count header, a table of receipts
//! (CAPSULE/ENTITY/DECISION/LABEL, decisions colored by verdict) and a footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::GateDecision;

use super::data::{EvidenceLensInput, EvidenceRow};

pub fn draw(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / capsule summary
            Constraint::Min(0),    // proof ledger
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_ledger(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn draw_header(f: &mut Frame, input: &EvidenceLensInput, area: Rect) {
    let text = format!(
        "Evidence — {} capsules · {} open · {} denied",
        input.total_capsules,
        input.open_capsules,
        input.denied(),
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
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = EvidenceLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
