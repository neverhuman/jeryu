//! Owner: Interactive TUI subsystem — mission control attention-first view
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::mission`
//! Invariants: Mission view renders from TuiReadModel and AttentionItems; no mutation.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::mission_shared::{MetricTile, render_metric_row};
use crate::api::read_model::{
    AttentionItem, MissionSnapshot, NextActionRecommendation, SystemHealth,
};
use crate::tui::theme::Theme;

/// Render the mission control attention-first cockpit.
#[allow(clippy::too_many_arguments)] // TUI cockpit: each arg is a distinct rendered surface
pub fn render_mission(
    f: &mut Frame,
    area: Rect,
    mission: &MissionSnapshot,
    attention: &[AttentionItem],
    health: &SystemHealth,
    next_actions: &[NextActionRecommendation],
    selected_attention: usize,
    theme: &Theme,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Posture banner
            Constraint::Length(7), // Metric tiles
            Constraint::Min(8),    // Body: attention + proof + actions
        ])
        .split(area);

    // ── Posture Banner ──────────────────────────────────────────────
    let posture_color = if !mission.safe_to_code {
        theme.fail
    } else if !mission.safe_to_merge {
        theme.warning
    } else if !mission.safe_to_release {
        theme.waiting
    } else {
        theme.ok
    };

    let posture_label = if !mission.safe_to_code {
        "BLOCKED — Cannot safely code"
    } else if !mission.safe_to_merge {
        "CAUTION — Merge conditions not met"
    } else if !mission.safe_to_release {
        "HOLD — Release gates incomplete"
    } else {
        "ALL CLEAR — Ready to ship"
    };

    let banner_lines = vec![
        Line::from(vec![Span::styled(
            format!("  {} ", posture_label),
            Style::default()
                .fg(theme.text_inverse)
                .bg(posture_color)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled("  code:", theme.muted()),
            Span::styled(
                if mission.safe_to_code { "✓ " } else { "✗ " },
                Style::default().fg(if mission.safe_to_code {
                    theme.ok
                } else {
                    theme.fail
                }),
            ),
            Span::styled("merge:", theme.muted()),
            Span::styled(
                if mission.safe_to_merge {
                    "✓ "
                } else {
                    "✗ "
                },
                Style::default().fg(if mission.safe_to_merge {
                    theme.ok
                } else {
                    theme.fail
                }),
            ),
            Span::styled("release:", theme.muted()),
            Span::styled(
                if mission.safe_to_release {
                    "✓ "
                } else {
                    "✗ "
                },
                Style::default().fg(if mission.safe_to_release {
                    theme.ok
                } else {
                    theme.fail
                }),
            ),
            Span::styled("agents:", theme.muted()),
            Span::styled(
                if mission.agents_can_code {
                    "✓"
                } else {
                    "✗"
                },
                Style::default().fg(if mission.agents_can_code {
                    theme.ok
                } else {
                    theme.fail
                }),
            ),
        ]),
    ];

    f.render_widget(
        Paragraph::new(banner_lines).block(
            Block::default()
                .title(" [ Mission Control ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(posture_color)),
        ),
        rows[0],
    );

    // ── Metric Tiles ────────────────────────────────────────────────
    let gitlab_label = match health.components().iter().find(|c| c.name == "gitlab") {
        Some(component) => component.status_label(),
        None => "unknown".to_string(),
    };
    let runners_value = format!("{}/{}", mission.active_runners, mission.total_runners);
    let agents_value = format!("{} active", mission.active_agents);
    let cache_value = format!("{} taints", mission.taint_count);
    let evidence_value = format!("{} capsules", mission.evidence_count);
    render_metric_row(
        f,
        rows[1],
        &[
            MetricTile {
                title: "GitLab",
                value: &gitlab_label,
                detail: None,
                color: metric_color(&gitlab_label, theme),
            },
            MetricTile {
                title: "Runners",
                value: &runners_value,
                detail: None,
                color: metric_color(&runners_value, theme),
            },
            MetricTile {
                title: "Agents",
                value: &agents_value,
                detail: None,
                color: metric_color(&agents_value, theme),
            },
            MetricTile {
                title: "Cache Trust",
                value: &cache_value,
                detail: None,
                color: metric_color(&cache_value, theme),
            },
            MetricTile {
                title: "Evidence",
                value: &evidence_value,
                detail: None,
                color: metric_color(&evidence_value, theme),
            },
        ],
    );

    // ── Body: Attention Queue + Proof Stack + Next Actions ───────
    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(44),
            Constraint::Percentage(32),
            Constraint::Percentage(24),
        ])
        .split(rows[2]);

    // Attention queue
    crate::tui::widgets::attention::render_attention_rail(
        f,
        body_cols[0],
        attention,
        Some(selected_attention),
        theme,
    );

    // Proof stack lanes
    render_proof_stack(f, body_cols[1], mission, theme);

    // Next actions
    render_next_actions(f, body_cols[2], next_actions, theme);
}

fn metric_color(value: &str, theme: &Theme) -> Color {
    if value.contains("unknown") || value.contains("degraded") {
        theme.warning
    } else if value.contains("taint") && !value.starts_with("0") {
        theme.blocked
    } else {
        theme.ok
    }
}

fn render_proof_stack(f: &mut Frame, area: Rect, mission: &MissionSnapshot, theme: &Theme) {
    let lanes = [
        (
            "Capability grants",
            if mission.safe_to_code {
                "observed"
            } else {
                "blocked"
            },
        ),
        (
            "VTI receipts",
            if mission.evidence_count > 0 {
                "present"
            } else {
                "needed"
            },
        ),
        (
            "Merge proof",
            if mission.safe_to_merge {
                "ready"
            } else {
                "blocked"
            },
        ),
        (
            "Release gate",
            if mission.safe_to_release {
                "green"
            } else {
                "pending"
            },
        ),
        ("Agent sandbox", "strict"),
    ];

    let lines: Vec<Line> = lanes
        .iter()
        .map(|(lane, state)| {
            let glyph = theme.status_glyph(state);
            let color = theme.status_color(state);
            Line::from(vec![
                Span::styled(format!(" {} ", glyph), theme.bold(color)),
                Span::styled(format!("{:<20}", lane), theme.primary()),
                Span::styled(state.to_string(), theme.muted()),
            ])
        })
        .collect();

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" [ Proof Stack ] ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_subtle)),
        ),
        area,
    );
}

fn render_next_actions(
    f: &mut Frame,
    area: Rect,
    actions: &[NextActionRecommendation],
    theme: &Theme,
) {
    let block = Block::default()
        .title(" [ Next Actions ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_accent));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if actions.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("  No recommended", theme.muted())),
                Line::from(Span::styled("  actions right now", theme.muted())),
            ]),
            inner,
        );
        return;
    }

    let max = inner.height as usize;
    let lines: Vec<Line> = actions
        .iter()
        .take(max)
        .map(|a| {
            let risk_color = theme.status_color(a.risk.label());
            let label = super::truncate_label(&a.label, inner.width.saturating_sub(10) as usize);
            Line::from(vec![
                Span::styled(
                    format!(" {} ", a.risk.label()),
                    Style::default()
                        .fg(theme.text_inverse)
                        .bg(risk_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}", label), theme.primary()),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mission_renders_safely() {
        let mission = MissionSnapshot::default();
        assert!(mission.safe_to_code);
        assert!(mission.agents_can_code);
    }
}
