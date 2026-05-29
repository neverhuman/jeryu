//! Owner: Interactive TUI subsystem - Workflow lens view
//! Proof: `cargo test -p jeryu --lib tui::lenses::workflow::view`
//! Invariants: Pure draw. Reads `WorkflowLensInput`; never touches DB,
//!             GitLab, Docker, Vault, filesystem, MCP, or network during
//!             render. Renders the Workflow Atlas: a fleet header, a colored
//!             per-repo delivery table, and a keys footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use super::data::{WorkflowLensInput, WorkflowRepoRow};
use crate::api::entity::HealthLevel;

pub fn draw(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // fleet header
            Constraint::Min(0),    // delivery table
            Constraint::Length(3), // keys footer
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_atlas(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn health_style(level: HealthLevel) -> Style {
    match level {
        HealthLevel::Healthy => Style::default().fg(Color::Green),
        HealthLevel::Warning => Style::default().fg(Color::Yellow),
        HealthLevel::Degraded => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        HealthLevel::Critical => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        HealthLevel::Unknown => Style::default().fg(Color::DarkGray),
    }
}

fn posture_style(posture: &str) -> Style {
    match posture {
        "FAILING" => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        "running" => Style::default().fg(Color::Green),
        "stale" => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Gray),
    }
}

fn draw_header(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let head = Span::raw(format!(
        "Workflow Atlas — {} repos · {} running · {} failing · health ",
        input.repo_count(),
        input.running_jobs,
        input.failed_jobs,
    ));
    let badge = Span::styled(input.overall.label(), health_style(input.overall));
    let line = ratatui::text::Line::from(vec![head, badge]);
    f.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Workflow Atlas — Delivery Flow "),
        ),
        area,
    );
}

fn draw_atlas(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    if input.repos.is_empty() {
        f.render_widget(
            Paragraph::new("No active delivery workflows.")
                .block(Block::default().borders(Borders::ALL).title(" Atlas ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("REPO"),
        Cell::from("FAMILY"),
        Cell::from("POSTURE"),
        Cell::from("RUNNING"),
        Cell::from("FAILING"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input.repos.iter().map(repo_row).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(16),
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Length(9),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Atlas — {} active ", input.active_count())),
    );

    f.render_widget(table, area);
}

fn repo_row(repo: &WorkflowRepoRow) -> Row {
    let failing = if repo.failed_count > 0 {
        Cell::from(Span::styled(
            repo.failed_count.to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else {
        Cell::from(repo.failed_count.to_string())
    };
    Row::new(vec![
        Cell::from(repo.alias.clone()),
        Cell::from(repo.family.clone()),
        Cell::from(Span::styled(
            repo.posture.clone(),
            posture_style(&repo.posture),
        )),
        Cell::from(repo.running_count.to_string()),
        failing,
    ])
}

fn draw_footer(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: enter drill · g graph · e evidence",
        input.event_cursor,
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Keys ")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::read_model::{RepoSummary, ReposSnapshot, TuiReadModel};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(w: u16, h: u16, input: &WorkflowLensInput) -> String {
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
        let model = TuiReadModel::default();
        let input = WorkflowLensInput::from_read_model(&model);
        let ink = render(80, 24, &input);
        assert!(!ink.trim().is_empty());
        assert!(ink.contains("Workflow Atlas"));
        assert!(ink.contains("No active delivery workflows."));
        assert!(ink.contains("cursor=0"));
    }

    #[test]
    fn renders_populated_at_120x36() {
        let mut model = TuiReadModel::default();
        let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
        repo.failed_count = 1;
        model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);
        model.mission.running_jobs = 3;
        model.mission.failed_jobs = 1;
        let input = WorkflowLensInput::from_read_model(&model);
        let ink = render(120, 36, &input);
        assert!(ink.contains("3 running"));
        assert!(ink.contains("1 failing"));
        assert!(ink.contains("core"));
        assert!(ink.contains("neverhuman"));
        assert!(ink.contains("FAILING"));
        assert!(ink.contains("Keys: enter drill"));
    }

    #[test]
    fn renders_no_placeholder_markers() {
        let model = TuiReadModel::default();
        let input = WorkflowLensInput::from_read_model(&model);
        let ink = render(120, 36, &input).to_lowercase();
        assert!(!ink.contains("placeholder"));
        assert!(!ink.contains("todo"));
        assert!(!ink.contains("land in u"));
        assert!(!ink.contains("lands in u"));
    }
}
