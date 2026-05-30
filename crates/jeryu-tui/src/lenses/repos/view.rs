//! Repos lens view.
//!
//! Invariants: pure draw. Reads [`ReposLensInput`]; no backend I/O.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::data::ReposLensInput;

pub fn draw(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(area);
    draw_fleet(f, input, rows[0]);

    let body = if area.width < 84 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(rows[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(24),
                Constraint::Percentage(26),
                Constraint::Percentage(26),
                Constraint::Percentage(24),
            ])
            .split(rows[1])
    };

    draw_families(f, input, body[0]);
    draw_repos(f, input, body[1]);
    draw_detail(f, input, body[2]);
    draw_attention(f, input, body[3]);
}

fn draw_fleet(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let counts = input.counts();
    let lines = vec![
        Line::from(format!(
            "families {}  repos {}  running {}  failed {}  aged {}",
            counts.families, counts.repos, counts.running, counts.failed, counts.aged
        )),
        Line::from(format!("registry {}", input.repos.registry_path)),
    ];
    f.render_widget(panel("Repository Fleet", lines), area);
}

fn draw_families(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let mut lines = Vec::new();
    for family in input
        .families()
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        lines.push(Line::from(format!(
            "{} {} r{} f{} a{}",
            family.name,
            family.status,
            family.running_count,
            family.failed_count,
            family.aged_count
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from("No repo families"));
    }
    f.render_widget(panel("Families", lines), area);
}

fn draw_repos(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let mut lines = Vec::new();
    for repo in input
        .repositories()
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        lines.push(Line::from(format!(
            "{} {} r{} f{}",
            repo.alias, repo.status, repo.running_count, repo.failed_count
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from("No repositories"));
    }
    f.render_widget(panel("Repositories", lines), area);
}

fn draw_detail(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let lines = if let Some(repo) = input.selected_repo() {
        vec![
            Line::from(format!("slug {}", repo.slug)),
            Line::from(format!("branch {}", repo.default_branch)),
            Line::from(format!(
                "local {} {} dirty:{}",
                repo.local_branch.as_deref().unwrap_or("-"),
                repo.local_sha.as_deref().unwrap_or("-"),
                repo.dirty
            )),
            Line::from(format!("next {}", repo.next_command)),
        ]
    } else {
        vec![Line::from("No repo selected")]
    };
    f.render_widget(panel("Repo Detail", lines), area);
}

fn draw_attention(f: &mut Frame, input: &ReposLensInput, area: Rect) {
    let counts = input.counts();
    let mut lines = Vec::new();
    if counts.failed > 0 {
        lines.push(Line::from(format!("P1 {} failed repo jobs", counts.failed)));
    }
    if counts.aged > 0 {
        lines.push(Line::from(format!("P2 {} aged repos", counts.aged)));
    }
    if lines.is_empty() {
        lines.push(Line::from("No scoped attention"));
    }
    f.render_widget(panel("Scoped Attention", lines), area);
}

fn panel<'a>(title: &'a str, lines: Vec<Line<'a>>) -> Paragraph<'a> {
    Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(Span::styled(
        format!(" {title} "),
        Style::default().add_modifier(Modifier::BOLD),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{RepoSummary, ReposSnapshot, TuiReadModel};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(w: u16, h: u16, input: &ReposLensInput) -> String {
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
    fn renders_default_repos_at_80x24() {
        let model = TuiReadModel::default();
        let input = ReposLensInput::from_read_model(&model);
        let ink = render(80, 24, &input);
        assert!(ink.contains("Repository Fleet"));
        assert!(ink.contains("No repositories"));
    }

    #[test]
    fn renders_populated_repos_at_120x36() {
        let mut model = TuiReadModel::default();
        let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
        repo.status = "running".into();
        repo.running_count = 1;
        model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);
        let input = ReposLensInput::from_read_model(&model);
        let ink = render(120, 36, &input);
        assert!(ink.contains("Families"));
        assert!(ink.contains("Repositories"));
        assert!(ink.contains("neverhuman"));
        assert!(ink.contains("core"));
    }
}
