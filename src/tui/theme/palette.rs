//! Owner: Interactive TUI subsystem — semantic palette (U12).
//! Proof: `cargo nextest run -p jeryu --lib tui::theme::palette`.
//! Invariants: every semantic name resolves to a `ratatui::style::Color`;
//! hex values come from TUI_RESET_PLAN_CLAUDE.md §6.6 and never drift silently.

use ratatui::style::Color;

/// Semantic color palette (truecolor-first). Each named field maps to a
/// `ratatui::style::Color`. The exact hex values are pinned by
/// `TUI_RESET_PLAN_CLAUDE.md §6.6 Theme + Glyphs + Badges` and are not
/// allowed to drift without a plan amendment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub ok: Color,       // #50FA7B
    pub queued: Color,   // #FFB86C
    pub running: Color,  // #8BE9FD
    pub warn: Color,     // #F1FA8C
    pub crit: Color,     // #FF5555
    pub agent: Color,    // #C792EA
    pub cache: Color,    // #7FDBCA
    pub vti: Color,      // #69DD8E
    pub release: Color,  // #FF79C6
    pub evidence: Color, // #F8C471
    pub stale: Color,    // #6E7681
    pub unknown: Color,  // #4A5057
}

impl Palette {
    /// Default dark palette — pinned to TUI_RESET_PLAN_CLAUDE.md §6.6 hex codes.
    pub const fn dark() -> Self {
        Self {
            ok: Color::Rgb(0x50, 0xFA, 0x7B),
            queued: Color::Rgb(0xFF, 0xB8, 0x6C),
            running: Color::Rgb(0x8B, 0xE9, 0xFD),
            warn: Color::Rgb(0xF1, 0xFA, 0x8C),
            crit: Color::Rgb(0xFF, 0x55, 0x55),
            agent: Color::Rgb(0xC7, 0x92, 0xEA),
            cache: Color::Rgb(0x7F, 0xDB, 0xCA),
            vti: Color::Rgb(0x69, 0xDD, 0x8E),
            release: Color::Rgb(0xFF, 0x79, 0xC6),
            evidence: Color::Rgb(0xF8, 0xC4, 0x71),
            stale: Color::Rgb(0x6E, 0x76, 0x81),
            unknown: Color::Rgb(0x4A, 0x50, 0x57),
        }
    }

    /// 16-color fallback for terminals without truecolor or 256 support.
    pub const fn fallback_16() -> Self {
        Self {
            ok: Color::Green,
            queued: Color::Yellow,
            running: Color::Cyan,
            warn: Color::LightYellow,
            crit: Color::Red,
            agent: Color::Magenta,
            cache: Color::LightCyan,
            vti: Color::LightGreen,
            release: Color::LightMagenta,
            evidence: Color::LightYellow,
            stale: Color::DarkGray,
            unknown: Color::Gray,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every named semantic resolves to a `Color`. Catches accidental
    /// deletions of a palette slot at the type level.
    #[test]
    fn semantic_colors_resolve() {
        let p = Palette::dark();
        // Touch every field — compile-time guarantees they exist; runtime
        // guarantees they are real Color values (not Reset).
        let all = [
            p.ok, p.queued, p.running, p.warn, p.crit, p.agent, p.cache, p.vti, p.release,
            p.evidence, p.stale, p.unknown,
        ];
        assert_eq!(all.len(), 12);
        for c in all {
            assert_ne!(c, Color::Reset, "semantic color must not be Reset");
        }
    }

    /// Hex values from TUI_RESET_PLAN_CLAUDE.md §6.6 must remain pinned.
    #[test]
    fn dark_palette_pins_plan_hex_codes() {
        let p = Palette::dark();
        assert_eq!(p.ok, Color::Rgb(0x50, 0xFA, 0x7B));
        assert_eq!(p.crit, Color::Rgb(0xFF, 0x55, 0x55));
        assert_eq!(p.running, Color::Rgb(0x8B, 0xE9, 0xFD));
        assert_eq!(p.queued, Color::Rgb(0xFF, 0xB8, 0x6C));
        assert_eq!(p.agent, Color::Rgb(0xC7, 0x92, 0xEA));
        assert_eq!(p.release, Color::Rgb(0xFF, 0x79, 0xC6));
        assert_eq!(p.evidence, Color::Rgb(0xF8, 0xC4, 0x71));
        assert_eq!(p.stale, Color::Rgb(0x6E, 0x76, 0x81));
    }

    /// 16-color fallback maps every semantic to a basic terminal color
    /// (never `Rgb`, which 16-color terminals cannot render).
    #[test]
    fn fallback_16_uses_basic_colors() {
        let p = Palette::fallback_16();
        assert_eq!(p.ok, Color::Green);
        assert_eq!(p.crit, Color::Red);
        assert_eq!(p.stale, Color::DarkGray);
        // Spot-check that no field accidentally kept an Rgb value.
        for c in [
            p.ok, p.queued, p.running, p.warn, p.crit, p.agent, p.cache, p.vti, p.release,
            p.evidence, p.stale, p.unknown,
        ] {
            assert!(
                !matches!(c, Color::Rgb(_, _, _)),
                "fallback must not use Rgb"
            );
        }
    }
}
