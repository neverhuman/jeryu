use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Widget,
};

use crate::tui::{
    lenses::repos::{ReposLensInput, ReposPane},
    theme::{TerminalCaps, Theme, status_badge},
    widgets::{shared::Panel, truncate_label},
};

#[derive(Debug, Clone, Copy)]
pub struct ReposLens<'a> {
    input: ReposLensInput<'a>,
    theme: &'a Theme,
    caps: TerminalCaps,
    active: ReposPane,
}

impl<'a> ReposLens<'a> {
    pub fn new(input: ReposLensInput<'a>, theme: &'a Theme, caps: TerminalCaps) -> Self {
        Self {
            input,
            theme,
            caps,
            active: ReposPane::Fleet,
        }
    }

    pub fn active(mut self, active: ReposPane) -> Self {
        self.active = active;
        self
    }
}

impl Widget for ReposLens<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(4)])
            .split(area);
        render_fleet(self.input, rows[0], buf, self.theme, self.caps, self.active);

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

        render_families(self.input, body[0], buf, self.theme, self.active);
        render_repos(self.input, body[1], buf, self.theme, self.active);
        render_detail(self.input, body[2], buf, self.theme, self.active);
        render_attention(self.input, body[3], buf, self.theme, self.active);
    }
}

fn render_fleet(
    input: ReposLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: ReposPane,
) {
    let counts = input.counts();
    let status = if counts.failed > 0 {
        "failed"
    } else if counts.running > 0 {
        "running"
    } else {
        "success"
    };
    Panel::new("Repository Fleet", theme)
        .active(active == ReposPane::Fleet, theme)
        .badge(status_badge(status, theme, caps))
        .line(Line::from(Span::styled(
            format!(
                "  families {}  repos {}  running {}  failed {}  aged {}",
                counts.families, counts.repos, counts.running, counts.failed, counts.aged
            ),
            theme.primary(),
        )))
        .line(Line::from(Span::styled(
            format!("  registry {}", input.model.repos.registry_path),
            theme.secondary(),
        )))
        .render(area, buf);
}

fn render_families(
    input: ReposLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: ReposPane,
) {
    let mut panel = Panel::new("Families", theme)
        .active(active == ReposPane::Families, theme)
        .empty("No repo families");
    for family in input
        .families()
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} {} r{} f{}",
                truncate_label(&family.name, 16),
                family.status,
                family.running_count,
                family.failed_count
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn render_repos(
    input: ReposLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: ReposPane,
) {
    let mut panel = Panel::new("Repositories", theme)
        .active(active == ReposPane::Repos, theme)
        .empty("No repositories");
    for repo in input
        .repos()
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} {} r{} f{}",
                truncate_label(&repo.alias, 16),
                repo.status,
                repo.running_count,
                repo.failed_count
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn render_detail(
    input: ReposLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: ReposPane,
) {
    let mut panel = Panel::new("Repo Detail", theme)
        .active(active == ReposPane::Detail, theme)
        .empty("No repo selected");
    if let Some(repo) = input.selected_repo() {
        panel = panel
            .line(Line::from(Span::styled(
                format!("  slug {}", truncate_label(&repo.slug, 36)),
                theme.primary(),
            )))
            .line(Line::from(Span::styled(
                format!("  branch {}", repo.default_branch),
                theme.secondary(),
            )))
            .line(Line::from(Span::styled(
                format!(
                    "  local {} {} dirty:{}",
                    repo.local_branch.as_deref().unwrap_or("-"),
                    repo.local_sha.as_deref().unwrap_or("-"),
                    repo.dirty
                ),
                theme.primary(),
            )))
            .line(Line::from(Span::styled(
                format!("  next {}", truncate_label(&repo.next_command, 42)),
                theme.secondary(),
            )));
    }
    panel.render(area, buf);
}

fn render_attention(
    input: ReposLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: ReposPane,
) {
    let attention = input.scoped_attention();
    let mut panel = Panel::new("Scoped Attention", theme)
        .active(active == ReposPane::Attention, theme)
        .empty("No scoped attention");
    for item in attention
        .iter()
        .take(area.height.saturating_sub(2) as usize)
    {
        panel = panel.line(Line::from(Span::styled(
            format!(
                "  {} {}",
                item.severity.label(),
                truncate_label(&item.title, 34)
            ),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}
