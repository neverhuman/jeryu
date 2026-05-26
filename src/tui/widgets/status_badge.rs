//! Owner: Interactive TUI subsystem — status badge rendering
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::status_badge`
//! Invariants: Badges are deterministic glyph+color pairs; no state mutation.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::tui::theme::{Badge, TerminalCaps, Theme};

/// Compute the badge for a raw status string.
pub fn badge_for_status(status: &str, theme: &Theme) -> Badge {
    crate::tui::theme::status_badge(status, theme, TerminalCaps::unicode())
}

/// Render a badge as a styled Span: "[PASS]" or "✓"
pub fn badge_span(status: &str, theme: &Theme) -> Span<'static> {
    let b = badge_for_status(status, theme);
    Span::styled(
        format!("[{}]", b.label),
        Style::default().fg(b.color).add_modifier(Modifier::BOLD),
    )
}

/// Render just the glyph as a styled Span: "✓" or "✗"
pub fn glyph_span(status: &str, theme: &Theme) -> Span<'static> {
    let b = badge_for_status(status, theme);
    Span::styled(b.glyph.to_string(), Style::default().fg(b.color))
}

// VTI-specific badges
pub fn vti_accelerated_badge(theme: &Theme) -> Span<'static> {
    Span::styled(
        "[VTI]",
        Style::default()
            .fg(theme.vti_fire)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn vti_skipped_badge(theme: &Theme) -> Span<'static> {
    Span::styled(
        "[SKIP]",
        Style::default()
            .fg(theme.skipped)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn vti_selected_badge(theme: &Theme) -> Span<'static> {
    Span::styled(
        "[SEL]",
        Style::default().fg(theme.ok).add_modifier(Modifier::BOLD),
    )
}

pub fn cache_hit_badge(theme: &Theme) -> Span<'static> {
    crate::tui::theme::cache_hit_badge(theme, TerminalCaps::unicode()).span()
}

pub fn cache_taint_badge(theme: &Theme) -> Span<'static> {
    crate::tui::theme::cache_taint_badge(theme, TerminalCaps::unicode()).span()
}

pub fn flake_badge(theme: &Theme) -> Span<'static> {
    crate::tui::theme::flake_badge(theme, TerminalCaps::unicode()).span()
}

pub fn agent_badge(theme: &Theme) -> Span<'static> {
    Span::styled(
        "[AGENT]",
        Style::default()
            .fg(theme.agent)
            .add_modifier(Modifier::BOLD),
    )
}

pub fn production_badge(theme: &Theme) -> Span<'static> {
    Span::styled(
        "[PROD]",
        Style::default()
            .fg(theme.production)
            .add_modifier(Modifier::BOLD),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_use_correct_theme_colors() {
        let t = Theme::dark();
        let pass = badge_for_status("success", &t);
        assert_eq!(pass.color, t.ok);
        assert_eq!(pass.glyph, "✓");

        let fail = badge_for_status("failed", &t);
        assert_eq!(fail.color, t.fail);
        assert_eq!(fail.glyph, "✗");
    }

    #[test]
    fn unknown_status_maps_to_info() {
        let t = Theme::dark();
        let b = badge_for_status("something-weird", &t);
        assert_eq!(b.label, "INFO");
    }
}
