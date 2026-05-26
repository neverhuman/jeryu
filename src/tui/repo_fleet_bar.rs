//! Owner: Interactive TUI subsystem — multi-repo fleet bar
//! Proof: `cargo test -p jeryu --lib tui::repo_fleet_bar`
//! Invariants: Fleet rendering is pure view logic; it never mutates repository or state-store data.

use crate::tui::app::App;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

pub fn draw_fleet_bar(f: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }

    let focused = app.focus.active == crate::tui::focus::PaneId::FleetBar;
    let items = app
        .state
        .fleet
        .scope_items(app.selected_repo_family.as_deref());

    let mut spans = Vec::new();

    // Breadcrumb prefix when drilled into a family
    if let Some(family) = &app.selected_repo_family {
        spans.push(Span::styled(
            format!(" {family} › "),
            Style::default().fg(Color::DarkGray),
        ));
    }

    for (idx, item) in items.iter().enumerate() {
        use crate::repo_fleet::FleetScopeKind;

        let selected = app.selected_repo_index == idx;
        let label = match item.kind {
            FleetScopeKind::All => {
                let r = &item.rollup;
                if r.repo_count == 0 {
                    " ALL: none ".to_string()
                } else {
                    format!(
                        " ALL run:{} fail:{} aged:{} ",
                        r.running_count, r.failed_count, r.aged_count
                    )
                }
            }
            FleetScopeKind::Family => {
                let r = &item.rollup;
                format!(
                    " {}({}) r{} f{} ",
                    item.label, r.repo_count, r.running_count, r.failed_count
                )
            }
            FleetScopeKind::Repo => {
                let r = &item.rollup;
                let mut l = format!(
                    " {} {} r{} f{}",
                    item.label, item.status, r.running_count, r.failed_count
                );
                if r.aged_count > 0 {
                    l.push_str(" aged");
                }
                if let Some(ri) = item.repo_index
                    && let Some(repo) = app.state.fleet.repos.get(ri)
                    && let Some(score) = repo.score_badge.as_deref()
                {
                    l.push_str(" score:");
                    l.push_str(score);
                }
                l.push(' ');
                l
            }
        };

        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(segment(label, selected, &item.status));
    }

    // Hint line at right edge
    let hint = if app.selected_repo_family.is_some() {
        "  Enter:scope ←→:choose Esc:back A:all "
    } else {
        "  Enter:drill ←→:choose Esc:all "
    };
    spans.push(Span::styled(hint, Style::default().fg(Color::DarkGray)));

    let line = Line::from(spans);
    if focused {
        // Focused: render with a yellow-tinted base style so it stands out
        // as the active pane without needing borders (bar is only 1 row).
        f.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Rgb(40, 40, 20))),
            area,
        );
    } else {
        f.render_widget(Paragraph::new(line), area);
    }
}

pub fn draw_repo_detail_overlay(f: &mut Frame, app: &mut App) {
    if !app.repo_detail_open {
        return;
    }
    let area = f.area();
    if area.width < 30 || area.height < 8 {
        return;
    }

    let overlay_w = (area.width * 2 / 3)
        .max(54)
        .min(area.width.saturating_sub(4));
    let overlay_h = 14.min(area.height.saturating_sub(2)).max(10);
    let overlay = Rect::new(
        area.x + (area.width.saturating_sub(overlay_w)) / 2,
        area.y + (area.height.saturating_sub(overlay_h)) / 2,
        overlay_w,
        overlay_h,
    );

    f.render_widget(Clear, overlay);
    let title = if let Some(repo) = app.selected_repo() {
        format!("Repo: {}", repo.alias)
    } else {
        "Repo: All".to_string()
    };

    let lines = if let Some(repo) = app.selected_repo() {
        let latest = repo.latest_run.as_ref();
        vec![
            Line::from(vec![
                Span::styled("Status  ", muted()),
                Span::styled(repo.status.clone(), status_style(&repo.status)),
                Span::raw(format!(
                    "  run:{} fail:{} aged:{}",
                    repo.running_count, repo.failed_count, repo.stale
                )),
            ]),
            Line::from(vec![
                Span::styled("Slug    ", muted()),
                Span::raw(repo.slug.clone()),
            ]),
            Line::from(vec![
                Span::styled("Local   ", muted()),
                Span::raw(format!(
                    "{} {} dirty:{}",
                    repo.local.branch.as_deref().unwrap_or("-"),
                    repo.local.sha_short.as_deref().unwrap_or("-"),
                    repo.local.dirty
                )),
            ]),
            Line::from(vec![
                Span::styled("Run     ", muted()),
                Span::raw(
                    latest
                        .and_then(|run| run.name.as_deref())
                        .unwrap_or("-")
                        .to_string(),
                ),
                Span::raw(" "),
                Span::raw(
                    latest
                        .and_then(|run| run.conclusion.as_deref().or(run.status.as_deref()))
                        .unwrap_or("-")
                        .to_string(),
                ),
            ]),
            Line::from(vec![
                Span::styled("Score   ", muted()),
                Span::raw(repo.score_badge.as_deref().unwrap_or("-").to_string()),
            ]),
            Line::from(vec![
                Span::styled("Cache   ", muted()),
                Span::raw(repo.cache_namespace.clone()),
            ]),
            Line::from(vec![
                Span::styled("Data    ", muted()),
                Span::raw(repo.data_namespace.clone()),
            ]),
            Line::from(vec![
                Span::styled("Next    ", muted()),
                Span::raw(repo.next_command.clone()),
            ]),
            Line::from(""),
            Line::from("Left/right or h/l selects repos. Esc returns to All."),
        ]
    } else {
        let (running, failed, aged) = app.state.fleet.counts();
        vec![
            Line::from(format!("Repositories: {}", app.state.fleet.repos.len())),
            Line::from(format!(
                "Running: {running}  Failed: {failed}  Aged: {aged}"
            )),
            Line::from(format!("Registry: {}", app.state.fleet.registry_path)),
            Line::from(""),
            Line::from("Enter opens repo detail. Left/right or h/l selects a repo."),
        ]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .title_top(Line::from(" [esc] ").right_aligned());
    f.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        overlay,
    );

    // Register the full title row as a clickable esc hotspot so users can
    // close the overlay with the mouse as well as the keyboard.
    app.focus_map.register_esc(
        crate::tui::focus::PaneId::FleetBar,
        Rect::new(overlay.x, overlay.y, overlay.width, 1),
    );
}

fn segment(label: String, selected: bool, status: &str) -> Span<'static> {
    let style = if selected {
        status_style(status)
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        status_style(status)
    };
    Span::styled(label, style)
}

fn status_style(status: &str) -> Style {
    match status {
        "green" | "success" => Style::default().fg(Color::Green),
        "running" => Style::default().fg(Color::Yellow),
        "failed" => Style::default().fg(Color::Red),
        "dirty" | "aged" => Style::default().fg(Color::Magenta),
        "missing" => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(Color::Cyan),
    }
}

fn muted() -> Style {
    Style::default().fg(Color::DarkGray)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[tokio::test]
    async fn fleet_bar_renders_all_and_selected_repo() -> anyhow::Result<()> {
        let mut app = crate::tui::app::test_app().await?;
        app.apply_demo_fixture();
        app.selected_repo_index = 1;
        let mut terminal = Terminal::new(TestBackend::new(120, 6))?;
        terminal.draw(|f| draw_fleet_bar(f, &app, f.area()))?;
        let text = rendered_text(&terminal);
        // Scope navigator uses "ALL" (uppercase)
        assert!(text.contains("ALL run:1 fail:1 aged:0"));
        assert!(text.contains("nht running r1 f0"));
        Ok(())
    }

    #[tokio::test]
    async fn fleet_bar_renders_family_chips_when_families_exist() -> anyhow::Result<()> {
        use crate::repo_fleet::{FleetRepoSnapshot, FleetSnapshot, RepoLocalStatus};

        let mut app = crate::tui::app::test_app().await?;
        // Inject a fleet with two repos sharing the "veox-" prefix
        app.state.fleet = FleetSnapshot {
            generated_at: "2026-01-01T00:00:00Z".into(),
            registry_path: ".jeryu/repos.toml".into(),
            repos: vec![
                FleetRepoSnapshot {
                    alias: "veox-nht".into(),
                    slug: "test/veox-nht".into(),
                    provider: "github".into(),
                    default_branch: "main".into(),
                    visibility: "private".into(),
                    health_profile: "default".into(),
                    status: "running".into(),
                    running_count: 1,
                    failed_count: 0,
                    stale: false,
                    score_badge: None,
                    local: RepoLocalStatus::default(),
                    latest_run: None,
                    next_command: String::new(),
                    family: Some("veox".into()),
                    last_activity_at: None,
                    cache_namespace: "jeryu-cache-v1-test__veox-nht".into(),
                    data_namespace: "jeryu-data-v1-test__veox-nht".into(),
                    utilization_pressure: 5,
                },
                FleetRepoSnapshot {
                    alias: "veox-shared".into(),
                    slug: "test/veox-shared".into(),
                    provider: "github".into(),
                    default_branch: "main".into(),
                    visibility: "private".into(),
                    health_profile: "default".into(),
                    status: "green".into(),
                    running_count: 0,
                    failed_count: 0,
                    stale: false,
                    score_badge: None,
                    local: RepoLocalStatus::default(),
                    latest_run: None,
                    next_command: String::new(),
                    family: Some("veox".into()),
                    last_activity_at: None,
                    cache_namespace: "jeryu-cache-v1-test__veox-shared".into(),
                    data_namespace: "jeryu-data-v1-test__veox-shared".into(),
                    utilization_pressure: 0,
                },
            ],
            events: Vec::new(),
        };
        app.selected_repo_index = 0;
        let mut terminal = Terminal::new(TestBackend::new(120, 3))?;
        terminal.draw(|f| draw_fleet_bar(f, &app, f.area()))?;
        let text = rendered_text(&terminal);
        // Should render the family chip "veox-*(2)" at root level
        assert!(
            text.contains("veox-*(2)"),
            "expected family chip 'veox-*(2)' in: {text:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn repo_detail_renders_selected_repo_context() -> anyhow::Result<()> {
        let mut app = crate::tui::app::test_app().await?;
        app.apply_demo_fixture();
        app.selected_repo_index = 2;
        app.repo_detail_open = true;
        // Taller terminal to accommodate the two new cache/data namespace rows
        let mut terminal = Terminal::new(TestBackend::new(100, 28))?;
        terminal.draw(|f| draw_repo_detail_overlay(f, &mut app))?;
        let text = rendered_text(&terminal);
        assert!(text.contains("Repo: shared"));
        assert!(text.contains("neverhuman/veox-shared"));
        assert!(text.contains("just fast"));
        // The overlay now shows cache and data namespace rows
        assert!(
            text.contains("jeryu-cache-v1-neverhuman__veox-shared"),
            "expected cache_namespace in overlay: {text:?}"
        );
        Ok(())
    }
}
