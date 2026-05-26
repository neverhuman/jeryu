use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::{
    api::freshness::SourceFreshness,
    tui::{
        action_registry::RiskTier,
        lenses::mission::{MissionLensInput, MissionPane},
        theme::{
            Badge, ProofConfidence, TerminalCaps, Theme, freshness_badge, proof_confidence_badge,
            status_badge,
        },
        widgets::{shared::Panel, truncate_label},
    },
};

#[derive(Debug, Clone, Copy)]
pub struct MissionLens<'a> {
    input: MissionLensInput<'a>,
    theme: &'a Theme,
    caps: TerminalCaps,
    active: MissionPane,
}

impl<'a> MissionLens<'a> {
    pub fn new(input: MissionLensInput<'a>, theme: &'a Theme, caps: TerminalCaps) -> Self {
        Self {
            input,
            theme,
            caps,
            active: MissionPane::Posture,
        }
    }

    pub fn active(mut self, active: MissionPane) -> Self {
        self.active = active;
        self
    }
}

impl Widget for MissionLens<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(6)])
            .split(area);
        render_posture(self.input, rows[0], buf, self.theme, self.caps, self.active);

        let body = if area.width < 70 {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(6),
                    Constraint::Length(6),
                    Constraint::Length(6),
                    Constraint::Min(4),
                ])
                .split(rows[1])
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                ])
                .split(rows[1])
        };

        render_blocker(self.input, body[0], buf, self.theme, self.active);
        render_freshness(self.input, body[1], buf, self.theme, self.caps, self.active);
        render_next_action(self.input, body[2], buf, self.theme, self.active);
        render_proofs(self.input, body[3], buf, self.theme, self.caps, self.active);
    }
}

fn render_posture(
    input: MissionLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: MissionPane,
) {
    let mission = input.mission();
    let badge = status_badge(input.posture_status(), theme, caps);
    Panel::new("Mission", theme)
        .active(active == MissionPane::Posture, theme)
        .badge(badge)
        .line(Line::from(Span::styled(
            format!("  {}", input.posture_label()),
            theme.bold(theme.status_color(input.posture_status())),
        )))
        .line(Line::from(vec![
            Span::styled("  code ", theme.muted()),
            Span::styled(
                yes_no(mission.safe_to_code),
                gate_style(mission.safe_to_code, theme),
            ),
            Span::styled("  merge ", theme.muted()),
            Span::styled(
                yes_no(mission.safe_to_merge),
                gate_style(mission.safe_to_merge, theme),
            ),
            Span::styled("  release ", theme.muted()),
            Span::styled(
                yes_no(mission.safe_to_release),
                gate_style(mission.safe_to_release, theme),
            ),
        ]))
        .render(area, buf);
}

fn render_blocker(
    input: MissionLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: MissionPane,
) {
    let mut panel = Panel::new("Top Blocker", theme)
        .active(active == MissionPane::TopBlocker, theme)
        .empty("No blockers");

    if let Some(blocker) = input.top_blocker() {
        let color = theme.severity_color(blocker.severity);
        panel = panel
            .line(Line::from(Span::styled(
                format!("  {} {}", blocker.severity.label(), blocker.summary),
                theme.bold(color),
            )))
            .line(Line::from(Span::styled(
                format!(
                    "  entity {}",
                    blocker
                        .entity
                        .as_ref()
                        .map_or("none".into(), |entity| entity.display())
                ),
                theme.secondary(),
            )));
    }

    panel.render(area, buf);
}

fn render_freshness(
    input: MissionLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: MissionPane,
) {
    match input.primary_freshness() {
        Some(source) => render_source_rows(source, area, buf, theme, caps, active),
        None => Panel::new("Freshness", theme)
            .active(active == MissionPane::Freshness, theme)
            .empty("No source status")
            .render(area, buf),
    }
}

fn render_source_rows(
    source: &SourceFreshness,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: MissionPane,
) {
    let badge = freshness_badge(source, theme, caps);
    let state_label = badge.label.clone();
    let source_label = format!("{:?}", source.source);
    let age = source
        .age_ms
        .map_or_else(|| "unknown".to_string(), compact_age);
    let cursor = source.cursor.as_deref().unwrap_or("none");
    Panel::new("Freshness", theme)
        .active(active == MissionPane::Freshness, theme)
        .badge(badge)
        .line(Line::from(Span::styled(
            format!("  {state_label}"),
            theme.bold(theme.status_color(source.state.badge())),
        )))
        .line(Line::from(Span::styled(
            format!("  source       {source_label}"),
            theme.primary(),
        )))
        .line(Line::from(Span::styled(
            format!("  state        {}", source.state.badge()),
            theme.primary(),
        )))
        .line(Line::from(Span::styled(
            format!("  age          {age}"),
            theme.primary(),
        )))
        .line(Line::from(Span::styled(
            format!("  cursor       {cursor}"),
            theme.primary(),
        )))
        .render(area, buf);
}

fn render_next_action(
    input: MissionLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    active: MissionPane,
) {
    let mut panel = Panel::new("Next Action", theme)
        .active(active == MissionPane::NextAction, theme)
        .empty("No recommended action");
    if let Some(action) = input.next_action() {
        panel = panel
            .badge(legacy_risk_badge(action.risk, theme))
            .line(Line::from(Span::styled(
                format!("  {}", action.action_ref.action_id),
                theme.bold(action.risk.color()),
            )))
            .line(Line::from(Span::styled(
                format!("  {}", truncate_label(&action.label, 48)),
                theme.primary(),
            )))
            .line(Line::from(Span::styled(
                format!("  proof confidence {:.0}%", action.confidence * 100.0),
                theme.secondary(),
            )));
    }
    panel.render(area, buf);
}

fn render_proofs(
    input: MissionLensInput<'_>,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    caps: TerminalCaps,
    active: MissionPane,
) {
    let proofs = input.proof_links();
    let badge = proof_confidence_badge(
        if proofs.is_empty() {
            ProofConfidence::Missing
        } else {
            ProofConfidence::Measured
        },
        theme,
        caps,
    );
    let mut panel = Panel::new("Proof Links", theme)
        .active(active == MissionPane::ProofLinks, theme)
        .badge(badge)
        .empty("No proof links");
    for proof in proofs.iter().take(area.height.saturating_sub(2) as usize) {
        panel = panel.line(Line::from(Span::styled(
            format!("  proof:{proof}"),
            theme.primary(),
        )));
    }
    panel.render(area, buf);
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn gate_style(value: bool, theme: &Theme) -> Style {
    let color = if value { theme.ok } else { theme.fail };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn legacy_risk_badge(risk: RiskTier, _theme: &Theme) -> Badge {
    Badge {
        glyph: "",
        label: risk.label().to_ascii_uppercase(),
        color: risk.color(),
        requires_proof: matches!(risk, RiskTier::High | RiskTier::Production),
    }
}

fn compact_age(age_ms: u64) -> String {
    if age_ms < 1_000 {
        format!("{age_ms}ms")
    } else if age_ms < 60_000 {
        format!("{}s", age_ms / 1_000)
    } else if age_ms < 3_600_000 {
        format!("{}m", age_ms / 60_000)
    } else {
        format!("{}h", age_ms / 3_600_000)
    }
}
