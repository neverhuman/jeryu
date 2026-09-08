//! Release lens view.
//!
//! Invariants: pure draw. Reads [`ReleaseLensInput`]; no backend I/O. Renders
//! release readiness: a posture header (safe-to-release + production health), a
//! per-candidate table (CANDIDATE/SHA/GATE/STAGE/SBOM, gate & SBOM colored), and
//! a promote/rollback/evidence footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::{HealthLevel, ReleaseGate, SbomStatus};

use super::data::{ReleaseLensInput, ReleaseRow};

pub fn draw(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // posture header
            Constraint::Min(0),    // candidate table or posture detail
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn health_style(level: HealthLevel) -> Style {
    match level {
        HealthLevel::Healthy => Style::default().fg(Color::Green),
        HealthLevel::Warning | HealthLevel::Degraded => Style::default().fg(Color::Yellow),
        HealthLevel::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        HealthLevel::Unknown => Style::default().fg(Color::DarkGray),
    }
}

fn gate_style(gate: ReleaseGate) -> Style {
    match gate {
        ReleaseGate::Ready => Style::default().fg(Color::Green),
        ReleaseGate::Pending => Style::default().fg(Color::Yellow),
        ReleaseGate::Blocked => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

fn sbom_style(sbom: SbomStatus) -> Style {
    match sbom {
        SbomStatus::Verified => Style::default().fg(Color::Green),
        SbomStatus::Present => Style::default().fg(Color::Cyan),
        SbomStatus::Missing => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

fn draw_header(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    let safe = if input.safe_to_release { "yes" } else { "NO" };
    let safe_style = if input.safe_to_release {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    let line = Line::from(vec![
        Span::raw("Release — safe_to_release="),
        Span::styled(safe, safe_style),
        Span::raw(" · production "),
        Span::styled(
            input.production_health.label(),
            health_style(input.production_health),
        ),
    ]);
    f.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Release — Readiness "),
        ),
        area,
    );
}

fn draw_body(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    if input.rows.is_empty() {
        draw_posture_detail(f, input, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("CANDIDATE"),
        Cell::from("SHA"),
        Cell::from("GATE"),
        Cell::from("STAGE"),
        Cell::from("SBOM"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input.rows.iter().map(candidate_row).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(18),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Release candidates "),
    );

    f.render_widget(table, area);
}

fn candidate_row(r: &ReleaseRow) -> Row<'_> {
    let sha: String = r.candidate_sha.chars().take(8).collect();
    Row::new(vec![
        Cell::from(r.label.clone()),
        Cell::from(sha),
        Cell::from(Span::styled(r.gate.label().to_string(), gate_style(r.gate))),
        Cell::from(r.stage.label().to_string()),
        Cell::from(Span::styled(r.sbom.label().to_string(), sbom_style(r.sbom))),
    ])
}

fn draw_posture_detail(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    let lines = vec![
        Line::from("No release candidates."),
        Line::from(format!(
            "Safe to release: {}",
            if input.safe_to_release { "yes" } else { "NO" }
        )),
        Line::from(vec![
            Span::raw("Production health: "),
            Span::styled(
                input.production_health.label(),
                health_style(input.production_health),
            ),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Release Posture "),
        ),
        area,
    );
}

fn draw_footer(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    let blocked = input.blocked();
    let line = if blocked > 0 {
        Line::from(Span::styled(
            format!(
                "⚠ {blocked} candidate(s) BLOCKED · cursor={} · Keys: p promote · r rollback · e evidence",
                input.event_cursor
            ),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(format!(
            "cursor={} · Keys: p promote · r rollback · e evidence",
            input.event_cursor
        ))
    };
    f.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{TuiReadModel, sample_read_model};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, input: &ReleaseLensInput) -> String {
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
        let input = ReleaseLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Release"));
        assert!(out.contains("safe_to_release"));
        assert!(out.contains("No release candidates."));
        assert!(out.contains("promote"));
        assert!(out.contains("rollback"));
    }

    #[test]
    fn renders_candidates_at_120x36() {
        let input = ReleaseLensInput::from_read_model(&sample_read_model());
        let out = ink(120, 36, &input);
        assert!(out.contains("GATE"));
        assert!(out.contains("SBOM"));
        assert!(out.contains("canary"));
        assert!(out.contains("verified"));
        assert!(out.contains("MISSING"));
        assert!(out.contains("BLOCKED"));
        assert!(out.contains("cursor=42"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = ReleaseLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
