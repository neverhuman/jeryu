//! Owner: Interactive TUI subsystem — universal entity inspector widget
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::inspector`
//! Invariants: Inspector renders from `EntityDetail`; never reaches into raw state.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::api::entity::{EntityDetail, Severity};
use crate::tui::theme::Theme;

/// Render the universal inspector for any selected entity.
/// This is the right-side pane that answers: What? Why? Proof? Actions? Related?
pub fn render_inspector(f: &mut Frame, area: Rect, detail: &EntityDetail, theme: &Theme) {
    let block = Block::default()
        .title(format!(
            " [ {} {} ] ",
            detail.entity.kind.label().to_uppercase(),
            detail.entity.id
        ))
        .borders(Borders::ALL)
        .border_style(
            detail
                .risk
                .map(|r| Style::default().fg(theme.status_color(r.label())))
                .unwrap_or(Style::default().fg(theme.border_active)),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // What: state + summary
            Constraint::Length(4), // Why: blockers
            Constraint::Length(4), // Proof: evidence
            Constraint::Length(4), // Actions
            Constraint::Min(1),    // Related
        ])
        .split(inner);

    // ── What ────────────────────────────────────────────────────────
    let mut what_lines = vec![Line::from(vec![
        Span::styled("  State: ", theme.muted()),
        Span::styled(&detail.state, theme.bold(theme.status_color(&detail.state))),
    ])];
    if !detail.summary.is_empty() {
        what_lines.push(Line::from(vec![
            Span::styled("  ", theme.muted()),
            Span::styled(
                truncate(
                    &detail.summary,
                    sections[0].width.saturating_sub(4) as usize,
                ),
                theme.secondary(),
            ),
        ]));
    }
    f.render_widget(
        Paragraph::new(what_lines).wrap(Wrap { trim: true }),
        sections[0],
    );

    // ── Why: blockers ───────────────────────────────────────────────
    let mut why_lines = vec![Line::from(Span::styled(
        "  Blockers",
        theme.bold(theme.text_secondary),
    ))];
    if detail.blockers.is_empty() {
        why_lines.push(Line::from(Span::styled(
            "  none",
            Style::default().fg(theme.ok),
        )));
    } else {
        for blocker in detail.blockers.iter().take(2) {
            let sev_color = match blocker.severity {
                Severity::Critical => theme.fail,
                Severity::Error => theme.warning,
                Severity::Warning => theme.waiting,
                Severity::Info => theme.text_muted,
            };
            why_lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", blocker.severity.label()),
                    theme.bold(sev_color),
                ),
                Span::styled(
                    truncate(
                        &blocker.summary,
                        sections[1].width.saturating_sub(8) as usize,
                    ),
                    theme.primary(),
                ),
            ]));
        }
    }
    f.render_widget(Paragraph::new(why_lines), sections[1]);

    // ── Proof: evidence ─────────────────────────────────────────────
    let mut proof_lines = vec![Line::from(Span::styled(
        "  Evidence",
        theme.bold(theme.text_secondary),
    ))];
    if detail.evidence.is_empty() {
        proof_lines.push(Line::from(Span::styled(
            "  no linked evidence",
            theme.muted(),
        )));
    } else {
        for ev in detail.evidence.iter().take(2) {
            proof_lines.push(Line::from(vec![
                Span::styled(format!("  [{}] ", ev.kind), theme.secondary()),
                Span::styled(ev.summary.as_deref().unwrap_or(&ev.id), theme.primary()),
            ]));
        }
    }
    f.render_widget(Paragraph::new(proof_lines), sections[2]);

    // ── Actions ─────────────────────────────────────────────────────
    let mut action_lines = vec![Line::from(Span::styled(
        "  Actions",
        theme.bold(theme.text_secondary),
    ))];
    if detail.available_actions.is_empty() {
        action_lines.push(Line::from(Span::styled(
            "  ^K palette for all actions",
            theme.muted(),
        )));
    } else {
        for action in detail.available_actions.iter().take(3) {
            let risk_color = action
                .risk
                .map(|r| theme.status_color(r.label()))
                .unwrap_or(theme.text_muted);
            action_lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", action.risk.map(|r| r.label()).unwrap_or("—")),
                    Style::default().fg(risk_color),
                ),
                Span::styled(&action.label, theme.primary()),
            ]));
        }
    }
    f.render_widget(Paragraph::new(action_lines), sections[3]);

    // ── Related ─────────────────────────────────────────────────────
    let mut rel_lines = vec![Line::from(Span::styled(
        "  Related",
        theme.bold(theme.text_secondary),
    ))];
    if detail.related.is_empty() {
        rel_lines.push(Line::from(Span::styled("  —", theme.muted())));
    } else {
        for entity in detail.related.iter().take(4) {
            rel_lines.push(Line::from(Span::styled(
                format!("  → {}", entity.display()),
                Style::default().fg(theme.agent),
            )));
        }
    }
    f.render_widget(Paragraph::new(rel_lines), sections[4]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max > 3 {
        format!("{}...", &s[..max - 3])
    } else {
        s[..max].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_strings() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }
}
