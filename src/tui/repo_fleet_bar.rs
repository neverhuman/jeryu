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

    let mut spans = Vec::new();
    let (running, failed, aged) = app.state.fleet.counts();
    let all_label = if app.state.fleet.repos.is_empty() {
        " All: none ".to_string()
    } else {
        format!(" All run:{} fail:{} aged:{} ", running, failed, aged)
    };
    spans.push(segment(all_label, app.selected_repo_index == 0, "local"));

    for (idx, repo) in app.state.fleet.repos.iter().enumerate() {
        let index = idx + 1;
        let mut label = format!(
            " {} {} r{} f{}",
            repo.alias, repo.status, repo.running_count, repo.failed_count
        );
        if repo.stale {
            label.push_str(" aged");
        }
        if let Some(score) = repo.score_badge.as_deref() {
            label.push_str(" score:");
            label.push_str(score);
        }
        label.push(' ');
        spans.push(Span::raw(" "));
        spans.push(segment(
            label,
            app.selected_repo_index == index,
            &repo.status,
        ));
    }

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

pub fn draw_repo_detail_overlay(f: &mut Frame, app: &App) {
    if !app.repo_detail_open {
        return;
    }
    let area = f.area();
    if area.width < 30 || area.height < 8 {
        return;
    }

    let overlay_w = (area.width * 2 / 3)
        .max(44)
        .min(area.width.saturating_sub(4));
    let overlay_h = 12.min(area.height.saturating_sub(2)).max(8);
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

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: true }),
        overlay,
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
        let mut terminal = Terminal::new(TestBackend::new(100, 6))?;
        terminal.draw(|f| draw_fleet_bar(f, &app, f.area()))?;
        let text = rendered_text(&terminal);
        assert!(text.contains("All run:1 fail:1 aged:0"));
        assert!(text.contains("nht running r1 f0"));
        Ok(())
    }

    #[tokio::test]
    async fn repo_detail_renders_selected_repo_context() -> anyhow::Result<()> {
        let mut app = crate::tui::app::test_app().await?;
        app.apply_demo_fixture();
        app.selected_repo_index = 2;
        app.repo_detail_open = true;
        let mut terminal = Terminal::new(TestBackend::new(100, 24))?;
        terminal.draw(|f| draw_repo_detail_overlay(f, &app))?;
        let text = rendered_text(&terminal);
        assert!(text.contains("Repo: shared"));
        assert!(text.contains("neverhuman/veox-shared"));
        assert!(text.contains("just fast"));
        Ok(())
    }
}
