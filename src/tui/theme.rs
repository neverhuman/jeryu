//! Owner: Interactive TUI subsystem — semantic theme system
//! Proof: `cargo nextest run -p jeryu -- tui::theme`
//! Invariants: All TUI rendering uses theme colors, never hardcoded Color values.
//! Theme provides accessible presets and consistent status-to-color mapping.

use ratatui::style::{Color, Modifier, Style};

use crate::api::entity::Severity;

// ── Theme ───────────────────────────────────────────────────────────────

/// Semantic color tokens for the entire TUI. Every rendering function
/// consumes a `Theme` reference instead of hardcoding `Color::*`.
#[derive(Debug, Clone)]
pub struct Theme {
    // Status semantics
    pub ok: Color,
    pub running: Color,
    pub waiting: Color,
    pub warning: Color,
    pub fail: Color,
    pub blocked: Color,
    pub skipped: Color,

    // Domain semantics
    pub security: Color,
    pub production: Color,
    pub agent: Color,
    pub vti_fire: Color,

    // Chrome
    pub selection: Color,
    pub border_subtle: Color,
    pub border_active: Color,
    pub border_accent: Color,
    pub inactive: Color,

    // Text
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_inverse: Color,

    // Backgrounds
    pub bg_primary: Color,
    pub bg_surface: Color,
    pub bg_highlight: Color,
}

impl Theme {
    /// The default dark theme — optimized for terminal readability.
    pub fn dark() -> Self {
        Self {
            ok: Color::Rgb(102, 204, 153),      // soft green
            running: Color::Rgb(102, 178, 255), // sky blue
            waiting: Color::Rgb(255, 204, 102), // warm amber
            warning: Color::Rgb(255, 178, 102), // orange
            fail: Color::Rgb(255, 102, 102),    // coral red
            blocked: Color::Rgb(204, 102, 255), // soft purple
            skipped: Color::Rgb(128, 128, 128), // gray

            security: Color::Rgb(255, 102, 178), // pink-red
            production: Color::Rgb(255, 80, 80), // strong red
            agent: Color::Rgb(102, 255, 255),    // electric cyan
            vti_fire: Color::Rgb(255, 165, 0),   // orange fire 🔥

            selection: Color::Rgb(0, 150, 200),    // deep cyan
            border_subtle: Color::Rgb(60, 60, 70), // charcoal
            border_active: Color::Rgb(102, 178, 255),
            border_accent: Color::Rgb(102, 255, 255),
            inactive: Color::Rgb(100, 100, 80), // brownish

            text_primary: Color::Rgb(230, 230, 230),
            text_secondary: Color::Rgb(170, 170, 180),
            text_muted: Color::Rgb(100, 100, 110),
            text_inverse: Color::Rgb(20, 20, 25),

            bg_primary: Color::Reset, // terminal default
            bg_surface: Color::Rgb(30, 30, 38),
            bg_highlight: Color::Rgb(45, 45, 55),
        }
    }

    /// High-contrast theme for accessibility.
    pub fn high_contrast() -> Self {
        Self {
            ok: Color::Green,
            running: Color::Cyan,
            waiting: Color::Yellow,
            warning: Color::Yellow,
            fail: Color::Red,
            blocked: Color::Magenta,
            skipped: Color::DarkGray,

            security: Color::Red,
            production: Color::LightRed,
            agent: Color::LightCyan,
            vti_fire: Color::LightYellow,

            selection: Color::White,
            border_subtle: Color::Gray,
            border_active: Color::White,
            border_accent: Color::LightCyan,
            inactive: Color::DarkGray,

            text_primary: Color::White,
            text_secondary: Color::Gray,
            text_muted: Color::DarkGray,
            text_inverse: Color::Black,

            bg_primary: Color::Reset,
            bg_surface: Color::Reset,
            bg_highlight: Color::DarkGray,
        }
    }

    // ── Style helpers ───────────────────────────────────────────────

    /// Map a raw status string to the semantic theme color.
    pub fn status_color(&self, status: &str) -> Color {
        match status {
            "success" | "passed" | "green" | "released" | "omitted" => self.ok,
            "running" | "in-flight" | "canary-authorized" => self.running,
            "pending"
            | "created"
            | "waiting"
            | "waiting_for_resource"
            | "preparing"
            | "ready-for-canary" => self.waiting,
            "failed" => self.fail,
            "blocked" | "blocked-by-upstream" => self.blocked,
            "canceled" | "vti-skipped" | "skipped" => self.skipped,
            _ => self.text_muted,
        }
    }

    /// Status glyph for inline badges.
    pub fn status_glyph(&self, status: &str) -> &'static str {
        match status {
            "success" | "passed" | "green" | "released" => "✓",
            "running" | "in-flight" => "●",
            "pending" | "created" | "waiting" | "waiting_for_resource" | "preparing" => "○",
            "failed" => "✗",
            "blocked" | "blocked-by-upstream" => "⊘",
            "canceled" | "vti-skipped" | "skipped" => "⊘",
            "manual" => "◇",
            _ => "·",
        }
    }

    pub fn bold(&self, color: Color) -> Style {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn muted(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    pub fn secondary(&self) -> Style {
        Style::default().fg(self.text_secondary)
    }

    pub fn primary(&self) -> Style {
        Style::default().fg(self.text_primary)
    }

    pub fn border_style_for(&self, active: bool) -> Style {
        if active {
            Style::default().fg(self.border_active)
        } else {
            Style::default().fg(self.border_subtle)
        }
    }

    pub fn severity_color(&self, severity: Severity) -> Color {
        match severity {
            Severity::Critical => self.fail,
            Severity::Error => self.warning,
            Severity::Warning => self.waiting,
            Severity::Info => self.text_muted,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_maps_statuses() {
        let t = Theme::dark();
        assert_eq!(t.status_color("success"), t.ok);
        assert_eq!(t.status_color("failed"), t.fail);
        assert_eq!(t.status_color("running"), t.running);
        assert_eq!(t.status_color("blocked"), t.blocked);
    }

    #[test]
    fn status_glyphs_are_distinct() {
        let t = Theme::dark();
        assert_ne!(t.status_glyph("success"), t.status_glyph("failed"));
        assert_ne!(t.status_glyph("running"), t.status_glyph("pending"));
    }

    #[test]
    fn high_contrast_uses_basic_colors() {
        let t = Theme::high_contrast();
        assert_eq!(t.ok, Color::Green);
        assert_eq!(t.fail, Color::Red);
    }
}
