use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::api::agent_session::AgentSession;
use crate::api::entity::Severity;
use crate::tui::theme::Theme;

pub(super) fn render_session_detail(f: &mut Frame, area: Rect, s: &AgentSession, theme: &Theme) {
    let state_color = super::state_theme_color(s.state, theme);

    let budget_bar = |used: f64, limit: f64| -> String {
        if limit <= 0.0 {
            return "n/a".to_string();
        }
        let pct = ((used / limit) * 100.0).min(100.0);
        let filled = (pct as usize * 10 / 100).min(10);
        format!(
            "{}{}  {:.0}%",
            "█".repeat(filled),
            "░".repeat(10 - filled),
            pct
        )
    };

    let time_bar = budget_bar(
        s.budget.time_used_secs as f64,
        s.budget.time_limit_secs as f64,
    );
    let ci_bar = budget_bar(
        s.budget.ci_minutes_used as f64,
        s.budget.ci_minutes_limit as f64,
    );

    let mut lines = vec![
        Line::from(vec![
            Span::styled("  State:  ", theme.muted()),
            Span::styled(
                format!("{} {}", s.state.glyph(), s.state.label()),
                theme.bold(state_color),
            ),
            Span::styled("  Trust: ", theme.muted()),
            Span::styled(
                s.trust_tier.label(),
                Style::default().fg(super::trust_color(s.trust_tier, theme)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Goal:   ", theme.muted()),
            Span::styled(
                super::super::truncate_label(&s.objective, area.width.saturating_sub(14) as usize),
                theme.primary(),
            ),
        ]),
    ];

    if let Some(ref intent) = s.current_intent {
        lines.push(Line::from(vec![
            Span::styled("  Intent: ", theme.muted()),
            Span::styled(intent.clone(), Style::default().fg(theme.running)),
        ]));
    }
    if let Some(ref step) = s.current_step {
        lines.push(Line::from(vec![
            Span::styled("  Step:   ", theme.muted()),
            Span::styled(step.clone(), theme.secondary()),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("  Time:   ", theme.muted()),
        Span::styled(
            time_bar,
            Style::default().fg(if s.budget.time_pct() > 80.0 {
                theme.fail
            } else {
                theme.ok
            }),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  CI:     ", theme.muted()),
        Span::styled(
            ci_bar,
            Style::default().fg(if s.budget.is_exhausted() {
                theme.fail
            } else {
                theme.ok
            }),
        ),
    ]));

    if let Some(conf) = s.confidence {
        lines.push(Line::from(vec![
            Span::styled("  Conf:   ", theme.muted()),
            Span::styled(
                format!("{:.0}%", conf * 100.0),
                Style::default().fg(if conf > 0.7 { theme.ok } else { theme.waiting }),
            ),
        ]));
    }

    if let Some(ref branch) = s.branch {
        lines.push(Line::from(vec![
            Span::styled("  Branch: ", theme.muted()),
            Span::styled(branch.clone(), theme.secondary()),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(format!(" [ {} ] ", s.id))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(state_color)),
        ),
        area,
    );
}

pub(super) fn render_agent_grants(f: &mut Frame, area: Rect, s: &AgentSession, theme: &Theme) {
    let mut lines = vec![
        Line::from(Span::styled(
            "  Authority",
            theme.bold(theme.text_secondary),
        )),
        Line::from(vec![
            Span::styled("  Trust: ", theme.muted()),
            Span::styled(
                s.trust_tier.label(),
                Style::default().fg(super::trust_color(s.trust_tier, theme)),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Active Grants",
            theme.bold(theme.text_secondary),
        )),
    ];

    if s.grants.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.muted())));
    } else {
        for g in s.grants.iter().take(4) {
            let remaining = g
                .expires_at
                .signed_duration_since(chrono::Utc::now())
                .num_seconds()
                .max(0);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", g.action_id),
                    Style::default().fg(theme.waiting),
                ),
                Span::styled(format!("{}s left", remaining), theme.muted()),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Blockers",
        theme.bold(theme.text_secondary),
    )));

    if s.blockers.is_empty() {
        lines.push(Line::from(Span::styled(
            "  clear",
            Style::default().fg(theme.ok),
        )));
    } else {
        for b in s.blockers.iter().take(3) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} ", b.severity.label()),
                    theme.bold(match b.severity {
                        Severity::Critical => theme.fail,
                        Severity::Error => theme.warning,
                        _ => theme.waiting,
                    }),
                ),
                Span::styled(
                    super::super::truncate_label(
                        &b.summary,
                        area.width.saturating_sub(10) as usize,
                    ),
                    theme.primary(),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Actions",
        theme.bold(theme.text_secondary),
    )));
    lines.push(Line::from(Span::styled(
        "  ^K explain blockers",
        theme.primary(),
    )));
    lines.push(Line::from(Span::styled(
        "  ^K fetch capsule",
        theme.primary(),
    )));

    if let Some(ref next) = s.next_action {
        lines.push(Line::from(vec![
            Span::styled("  → ", Style::default().fg(theme.running)),
            Span::styled(&next.label, theme.bold(theme.running)),
        ]));
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" [ Grants / Blockers ] ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.blocked)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}
