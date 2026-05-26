//! Owner: Interactive TUI subsystem — theme module root (U12).
//! Proof: `cargo nextest run -p jeryu --lib tui::theme::`.
//! Invariants: theme module groups palette/glyphs/badges/progress/terminal_caps
//! plus the legacy `Theme` struct preserved via `mod legacy` re-export so the
//! existing 30+ `crate::tui::theme::Theme` callers keep compiling unchanged.

pub mod badges;
pub mod glyphs;
pub mod palette;
pub mod progress;
pub mod terminal_caps;

// Legacy `Theme` struct (status-color and severity helpers). Public re-export
// preserves the historical `crate::tui::theme::Theme` API surface.
mod legacy;
pub use legacy::Theme;

use crate::tui::theme::glyphs::Glyph;
use crate::tui::theme::palette::Palette;
use crate::tui::theme::terminal_caps::TerminalCaps;

/// Aggregate theme view-model bundling a `Palette`, a unicode/ASCII glyph
/// switch (driven by `TerminalCaps`), and the legacy `Theme` for callers
/// that still depend on it. Constructed at startup; pure thereafter.
#[derive(Debug, Clone)]
pub struct ThemeBundle {
    pub palette: Palette,
    pub caps: TerminalCaps,
    pub legacy: Theme,
}

impl ThemeBundle {
    pub fn dark(caps: TerminalCaps) -> Self {
        Self {
            palette: Palette::dark(),
            caps,
            legacy: Theme::dark(),
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
