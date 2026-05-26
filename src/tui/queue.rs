//! Owner: Interactive TUI subsystem — unified "All Active Work" queue
//! Proof: `cargo nextest run -p jeryu -- tui::ui_panels_body_queue`
//! Invariants: Queue rendering is pure view logic; it never mutates repository or state-store data.

use crate::tui::app::App;
use crate::tui::workflow::model::{CanonicalPhase, PrStatus, PullRequestView};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Render the unified "All Active Work" queue — shown when the Workflow tab
/// is active and the scope is `RepoFilter::All` (no specific repo selected).
///
/// Displays every PR in the delivery snapshot sorted by urgency (running →
/// failed → waiting → merged), with status glyphs, display numbers, titles,
/// and right-aligned repo aliases.
pub fn draw_all_queue(f: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let focused = app.focus.active == crate::tui::focus::PaneId::QueueAll;
    let border_color = if focused { Color::Yellow } else { Color::DarkGray };

    let prs = &app.delivery_snapshot.pull_requests;

    // Build sorted indices: running first, then failed, then open, then merged.
    let mut indices: Vec<usize> = (0..prs.len()).collect();
    indices.sort_by_key(|&i| sort_key(&prs[i]));

    // Stats
    let running = prs.iter().filter(|pr| pr.status == PrStatus::Running).count();
    let failed = prs
        .iter()
        .filter(|pr| pr.status == PrStatus::Blocked)
        .count();
    let waiting = prs
        .iter()
        .filter(|pr| matches!(pr.status, PrStatus::Open | PrStatus::Draft))
        .count();
    let total = prs.len();

    let title = format!(
        " All Active Work — run:{running} fail:{failed} wait:{waiting} total:{total} "
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if prs.is_empty() {
        let msg = Line::from(Span::styled(
            "  No active pull requests across any repo.",
            Style::default().fg(Color::DarkGray),
        ));
        f.render_widget(Paragraph::new(vec![msg]), inner);
        return;
    }

    let visible_h = inner.height as usize;
    let selected = app.delivery_snapshot.selected_pr_idx;
    let max_w = inner.width as usize;

    // Auto-scroll: ensure the selected PR row is visible.
    let selected_sort_pos = indices.iter().position(|&i| i == selected).unwrap_or(0);
    let scroll_offset = if selected_sort_pos >= visible_h {
        selected_sort_pos.saturating_sub(visible_h / 2)
    } else {
        0
    };

    let mut lines: Vec<Line<'_>> = Vec::with_capacity(visible_h);
    for (_sort_idx, &pr_idx) in indices.iter().enumerate().skip(scroll_offset).take(visible_h) {
        let pr = &prs[pr_idx];
        let is_selected = pr_idx == selected;

        let glyph = status_glyph(pr);
        let glyph_color = status_color(pr);
        let display_num = pr.display_number();
        let repo = pr.repo_alias.as_deref().unwrap_or("—");

        // Layout: " G DISPLAY_NUM  TITLE...  REPO "
        // Reserve space for: 1 (space) + glyph(1-2) + 1 (space) + display_num + 2 (gap) + repo + 1 (trailing space)
        let fixed_w = 1 + 2 + 1 + display_num.len() + 2 + repo.len() + 1;
        let title_budget = max_w.saturating_sub(fixed_w);
        let short_title = if pr.title.len() > title_budget && title_budget > 1 {
            format!("{}…", &pr.title[..title_budget.saturating_sub(1)])
        } else {
            pr.title.clone()
        };

        // Pad title to right-align repo alias
        let title_pad = title_budget.saturating_sub(short_title.len());

        let line_style = if is_selected && focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(50, 50, 60))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let spans = vec![
            Span::styled(format!(" {glyph} "), Style::default().fg(glyph_color)),
            Span::styled(
                format!("{display_num:<width$}", width = 8.max(display_num.len())),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(short_title, Style::default().fg(Color::White)),
            Span::raw(" ".repeat(title_pad)),
            Span::styled(
                format!(" {repo} "),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ),
        ];

        // Apply line-level selection style by wrapping spans
        if is_selected {
            lines.push(Line::from(
                spans
                    .into_iter()
                    .map(|s| Span::styled(s.content, s.style.patch(line_style)))
                    .collect::<Vec<_>>(),
            ));
        } else {
            lines.push(Line::from(spans));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn sort_key(pr: &PullRequestView) -> (u8, u8) {
    let status_order = match pr.status {
        PrStatus::Running => 0,
        PrStatus::Blocked => 1,
        PrStatus::Open => 2,
        PrStatus::Draft => 3,
        PrStatus::Merged => 4,
        PrStatus::Closed => 5,
    };
    let phase_order = match pr.phase {
        CanonicalPhase::PromoteProd | CanonicalPhase::MonitorRollback => 0,
        CanonicalPhase::PostMergeCI | CanonicalPhase::AgentReviewPostMerge => 1,
        CanonicalPhase::AutoMerge => 2,
        CanonicalPhase::PreMergeCI | CanonicalPhase::AgentReviewPreMerge => 3,
        CanonicalPhase::BuildArtifact | CanonicalPhase::PromoteLocal | CanonicalPhase::PromoteDev => 4,
    };
    (status_order, phase_order)
}

fn status_glyph(pr: &PullRequestView) -> &'static str {
    match pr.status {
        PrStatus::Running => "●",
        PrStatus::Blocked => "✗",
        PrStatus::Open => "○",
        PrStatus::Draft => "◌",
        PrStatus::Merged => "◉",
        PrStatus::Closed => "—",
    }
}

fn status_color(pr: &PullRequestView) -> Color {
    match pr.status {
        PrStatus::Running => Color::Green,
        PrStatus::Blocked => Color::Red,
        PrStatus::Open => Color::Yellow,
        PrStatus::Draft => Color::DarkGray,
        PrStatus::Merged => Color::Cyan,
        PrStatus::Closed => Color::DarkGray,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[tokio::test]
    async fn queue_renders_all_prs_with_status_glyphs() -> anyhow::Result<()> {
        let mut app = crate::tui::app::test_app().await?;
        app.apply_demo_fixture();
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        terminal.draw(|f| draw_all_queue(f, &app, f.area()))?;
        let text = rendered_text(&terminal);
        assert!(
            text.contains("All Active Work"),
            "queue should show title: {text:?}"
        );
        // The demo fixture has 5 PRs — check we see at least some display numbers
        assert!(
            text.contains("#1842") || text.contains("#1841"),
            "queue should render PR numbers: {text:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn queue_empty_state_renders_message() -> anyhow::Result<()> {
        let mut app = crate::tui::app::test_app().await?;
        let mut terminal = Terminal::new(TestBackend::new(80, 20))?;
        terminal.draw(|f| draw_all_queue(f, &app, f.area()))?;
        let text = rendered_text(&terminal);
        assert!(
            text.contains("No active pull requests"),
            "empty queue: {text:?}"
        );
        Ok(())
    }
}
