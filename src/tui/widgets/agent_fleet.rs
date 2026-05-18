//! Owner: Interactive TUI subsystem — agent fleet cockpit widget
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::agent_fleet`
//! Invariants: Agent fleet view renders from AgentSession; no control-plane mutation.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::api::agent_session::{AgentSession, AgentState, PatchStatus, TrustTier};
use crate::tui::theme::Theme;

#[path = "agent_fleet_panels.rs"]
mod agent_fleet_panels;

/// Render the full agent fleet view — replaces pipeline-centric agent rendering.
pub fn render_agent_fleet(
    f: &mut Frame,
    area: Rect,
    sessions: &[AgentSession],
    selected: usize,
    theme: &Theme,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34), // Agent list
            Constraint::Percentage(40), // Agent detail cockpit
            Constraint::Percentage(26), // Grants + actions
        ])
        .split(area);

    // ── Agent List ──────────────────────────────────────────────────
    let items: Vec<ListItem> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_selected = i == selected;
            let prefix = if is_selected { ">>" } else { "  " };
            let state_color = state_theme_color(s.state, theme);

            let line = Line::from(vec![
                Span::styled(
                    format!("{} {} ", prefix, s.state.glyph()),
                    Style::default()
                        .fg(state_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<5} ", s.state.label()),
                    Style::default().fg(state_color),
                ),
                Span::styled(
                    super::truncate_label(&s.objective, cols[0].width.saturating_sub(16) as usize),
                    if is_selected {
                        theme.primary()
                    } else {
                        theme.secondary()
                    },
                ),
            ]);

            let style = if is_selected {
                Style::default().bg(Color::Rgb(45, 45, 55))
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();

    let count_active = sessions.iter().filter(|s| s.state.is_active()).count();
    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(format!(
                    " [ Agent Fleet ({}/{}) ] ",
                    count_active,
                    sessions.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.agent)),
        ),
        cols[0],
    );

    // ── Agent Detail Cockpit ────────────────────────────────────────
    let detail_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // Detail card
            Constraint::Min(6),     // Patch race board
        ])
        .split(cols[1]);

    if let Some(session) = sessions.get(selected) {
        agent_fleet_panels::render_session_detail(f, detail_rows[0], session, theme);
        render_patch_board(f, detail_rows[1], session, theme);
    } else {
        f.render_widget(
            Paragraph::new(
                "  No agent sessions yet.\n  Branches starting with agent/ appear here.",
            )
            .block(
                Block::default()
                    .title(" [ Agent Cockpit ] ")
                    .borders(Borders::ALL),
            )
            .style(theme.muted()),
            detail_rows[0],
        );
    }

    // ── Grants + Actions ────────────────────────────────────────────
    if let Some(session) = sessions.get(selected) {
        agent_fleet_panels::render_agent_grants(f, cols[2], session, theme);
    } else {
        f.render_widget(
            Paragraph::new("  —").block(
                Block::default()
                    .title(" [ Grants ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_subtle)),
            ),
            cols[2],
        );
    }
}

fn render_patch_board(f: &mut Frame, area: Rect, s: &AgentSession, theme: &Theme) {
    let block = Block::default()
        .title(format!(" [ Patch Race ({}) ] ", s.patch_attempts.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if s.patch_attempts.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  No patches yet", theme.muted())),
            inner,
        );
        return;
    }

    let max = inner.height as usize;
    let lines: Vec<Line> = s
        .patch_attempts
        .iter()
        .take(max)
        .map(|p| {
            let status_color = patch_color(p.status, theme);
            let score_str = match p.score {
                Some(score) => format!(" score:{}", score),
                None => String::new(),
            };
            Line::from(vec![
                Span::styled(format!(" {} ", p.status.glyph()), theme.bold(status_color)),
                Span::styled(
                    super::truncate_label(&p.label, inner.width.saturating_sub(30) as usize),
                    theme.primary(),
                ),
                Span::styled(
                    format!(" {} {}{}", p.diff_stat, p.risk.label(), score_str),
                    theme.muted(),
                ),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn state_theme_color(state: AgentState, theme: &Theme) -> Color {
    match state {
        AgentState::Spawning => theme.waiting,
        AgentState::Diagnosing => theme.running,
        AgentState::Patching => theme.agent,
        AgentState::Validating => theme.running,
        AgentState::Racing => theme.vti_fire,
        AgentState::Blocked | AgentState::Failed => theme.fail,
        AgentState::WaitingApproval => theme.warning,
        AgentState::Completed => theme.ok,
        AgentState::Paused => theme.skipped,
    }
}

fn trust_color(tier: TrustTier, theme: &Theme) -> Color {
    match tier {
        TrustTier::Untrusted => theme.fail,
        TrustTier::Standard => theme.waiting,
        TrustTier::Trusted => theme.ok,
        TrustTier::Elevated => theme.blocked,
    }
}

fn patch_color(status: PatchStatus, theme: &Theme) -> Color {
    match status {
        PatchStatus::Proposed => theme.waiting,
        PatchStatus::Testing => theme.running,
        PatchStatus::Green => theme.ok,
        PatchStatus::Failed => theme.fail,
        PatchStatus::Winner => theme.vti_fire,
        PatchStatus::Archived => theme.skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_colors_are_themed() {
        let t = Theme::dark();
        assert_eq!(state_theme_color(AgentState::Completed, &t), t.ok);
        assert_eq!(state_theme_color(AgentState::Failed, &t), t.fail);
        assert_eq!(state_theme_color(AgentState::Racing, &t), t.vti_fire);
    }

    #[test]
    fn trust_colors_are_themed() {
        let t = Theme::dark();
        assert_eq!(trust_color(TrustTier::Trusted, &t), t.ok);
        assert_eq!(trust_color(TrustTier::Untrusted, &t), t.fail);
    }
}
