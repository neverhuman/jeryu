//! Owner: Interactive TUI subsystem - Release lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::release::view`
//! Invariants: Pure draw. Reads `ReleaseLensInput`; no I/O.
//!             Renders release readiness: a posture header, a per-repo
//!             readiness table (or posture detail rows when no repos are
//!             tracked), and a footer with promote/rollback/evidence keys.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::{ReleaseLensInput, ReleaseRepoRow};
use crate::api::entity::HealthLevel;

pub fn draw(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // posture header
            Constraint::Min(0),    // repo table or posture detail
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn posture_style(overall: HealthLevel) -> Style {
    match overall {
        HealthLevel::Healthy => Style::default().fg(Color::Green),
        HealthLevel::Warning => Style::default().fg(Color::Yellow),
        HealthLevel::Degraded => Style::default().fg(Color::Yellow),
        HealthLevel::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        HealthLevel::Unknown => Style::default().fg(Color::DarkGray),
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
        Span::raw(" · posture "),
        Span::styled(input.overall.label(), posture_style(input.overall)),
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

fn readiness_style(row: &ReleaseRepoRow) -> Style {
    match row.readiness() {
        "BLOCKED" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        "stale" => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Green),
    }
}

fn draw_body(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    if input.repos.is_empty() {
        draw_posture_detail(f, input, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("REPO"),
        Cell::from("SLUG"),
        Cell::from("STATUS"),
        Cell::from("READINESS"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .repos
        .iter()
        .map(|r| {
            Row::new(vec![
                Cell::from(r.alias.clone()),
                Cell::from(r.slug.clone()),
                Cell::from(r.status.clone()),
                Cell::from(Span::styled(r.readiness().to_string(), readiness_style(r))),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Release-tracked repositories "),
    );

    f.render_widget(table, area);
}

fn draw_posture_detail(f: &mut Frame, input: &ReleaseLensInput, area: Rect) {
    let lines = vec![
        Line::from("No release-tracked repositories."),
        Line::from(format!(
            "Safe to release: {}",
            if input.safe_to_release { "yes" } else { "NO" }
        )),
        Line::from(vec![
            Span::raw("Overall posture: "),
            Span::styled(input.overall.label(), posture_style(input.overall)),
        ]),
        Line::from(format!("Open capsules: {}", input.open_capsules)),
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
    let blocked = input.blocked_repos();
    let line = if blocked > 0 {
        Line::from(Span::styled(
            format!(
                "⚠ {blocked} repo(s) BLOCKED · cursor={} · Keys: p promote · r rollback · e evidence",
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
    use crate::api::read_model::{RepoSummary, ReposSnapshot, TuiReadModel};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(w: u16, h: u16, input: &ReleaseLensInput) -> String {
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

    fn populated_input() -> ReleaseLensInput {
        let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
        repo.status = "failed".into();
        repo.failed_count = 1;
        let mut model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);
        ReleaseLensInput::from_read_model(&model)
    }

    #[test]
    fn renders_empty_state_at_80x24() {
        let input = ReleaseLensInput::from_read_model(&TuiReadModel::default());
        let ink = render(80, 24, &input);
        assert!(ink.contains("Release"));
        assert!(ink.contains("safe_to_release"));
        assert!(ink.contains("No release-tracked repositories"));
        assert!(ink.contains("promote"));
        assert!(ink.contains("rollback"));
        assert!(ink.contains("evidence"));
    }

    #[test]
    fn renders_repo_table_with_readiness_at_120x36() {
        let input = populated_input();
        let ink = render(120, 36, &input);
        assert!(ink.contains("core"), "repo alias must render");
        assert!(ink.contains("neverhuman/jeryu"), "repo slug must render");
        assert!(ink.contains("READINESS"), "table header must render");
        assert!(ink.contains("BLOCKED"), "failed repo blocks release");
        assert!(ink.contains("cursor=42"), "footer must carry the cursor");
    }

    #[test]
    fn renders_at_220x60() {
        let input = populated_input();
        render(220, 60, &input);
    }
}
