//! Theme module root — palette / glyphs / badges / terminal capabilities.
//!
//! Invariants: the [`ThemeBundle`] bundles a [`Palette`], a unicode/ASCII glyph
//! switch (driven by [`TerminalCaps`]), and a status-string [`Theme`] for
//! surfaces that color by status string. Constructed at startup; pure thereafter.

pub mod badges;
pub mod glyphs;
pub mod palette;
pub mod terminal_caps;

use jeryu_readmodel::HealthLevel;
use ratatui::style::Color;

use glyphs::Glyph;
use palette::Palette;
use terminal_caps::TerminalCaps;

/// Status-string color theme. Resolves [`HealthLevel`] (read-model contract)
/// and queue states onto palette colors. Used by the chrome/status surfaces
/// that style by status string rather than by typed badge.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub palette: Palette,
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            palette: Palette::dark(),
        }
    }

    /// Color for an overall [`HealthLevel`] posture.
    pub fn health_color(&self, level: HealthLevel) -> Color {
        match level {
            HealthLevel::Healthy => self.palette.ok,
            HealthLevel::Warning => self.palette.warn,
            HealthLevel::Degraded => self.palette.queued,
            HealthLevel::Critical => self.palette.crit,
            HealthLevel::Unknown => self.palette.unknown,
        }
    }

    /// Color for a CI/job status string (`running`/`failed`/`passed`/…).
    pub fn status_color(&self, status: &str) -> Color {
        match status {
            "running" | "active" => self.palette.running,
            "queued" | "pending" | "created" => self.palette.queued,
            "passed" | "success" | "green" => self.palette.ok,
            "failed" | "error" => self.palette.crit,
            "warning" | "warn" => self.palette.warn,
            "expired" | "aged" => self.palette.muted,
            _ => self.palette.unknown,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Aggregate theme view-model bundling a [`Palette`], a glyph capability switch
/// driven by [`TerminalCaps`], and the status-string [`Theme`].
#[derive(Debug, Clone)]
pub struct ThemeBundle {
    pub palette: Palette,
    pub caps: TerminalCaps,
    pub status_theme: Theme,
}

impl ThemeBundle {
    pub fn dark(caps: TerminalCaps) -> Self {
        Self {
            palette: Palette::dark(),
            caps,
            status_theme: Theme::dark(),
        }
    }

    /// Render a glyph using the bundled terminal capability flag.
    pub fn glyph(&self, g: Glyph) -> &'static str {
        g.render(self.caps.supports_unicode)
    }
}

impl Default for ThemeBundle {
    fn default() -> Self {
        Self::dark(TerminalCaps::truecolor_unicode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_glyph_respects_unicode_capability() {
        let unicode = ThemeBundle::dark(TerminalCaps::truecolor_unicode());
        let ascii = ThemeBundle::dark(TerminalCaps::ascii_no_color());
        assert_eq!(unicode.glyph(Glyph::Passed), "✓");
        assert_eq!(ascii.glyph(Glyph::Passed), "v");
    }

    #[test]
    fn health_color_maps_every_level() {
        let t = Theme::dark();
        assert_eq!(t.health_color(HealthLevel::Healthy), t.palette.ok);
        assert_eq!(t.health_color(HealthLevel::Critical), t.palette.crit);
        assert_eq!(t.health_color(HealthLevel::Unknown), t.palette.unknown);
    }

    #[test]
    fn status_color_maps_known_states() {
        let t = Theme::dark();
        assert_eq!(t.status_color("running"), t.palette.running);
        assert_eq!(t.status_color("failed"), t.palette.crit);
        assert_eq!(t.status_color("mystery"), t.palette.unknown);
    }
}
