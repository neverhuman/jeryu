//! Semantic glyph inventory (ported from the source TUI theme).
//!
//! Invariants: every `Glyph` variant returns a non-empty unicode string AND a
//! non-empty ASCII fallback.

/// Semantic glyph inventory. Each variant has a unicode rendering and an ASCII
/// fallback for terminals without unicode support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[rustfmt::skip]
pub enum Glyph {
    Running, Active, Queued, Waiting,
    Passed, Failed, Blocked, Skipped,
    Proof, Paused, Grant, Release,
    Warn, Stale,
}

impl Glyph {
    /// Unicode glyph (default rendering).
    #[rustfmt::skip]
    pub fn unicode(self) -> &'static str {
        match self {
            Self::Running => "●", Self::Active  => "▶", Self::Queued  => "○", Self::Waiting => "…",
            Self::Passed  => "✓", Self::Failed  => "✗", Self::Blocked => "⛔", Self::Skipped => "↷",
            Self::Proof   => "◆", Self::Paused  => "⏸", Self::Grant   => "⚿", Self::Release => "⬢",
            Self::Warn    => "!", Self::Stale   => "~",
        }
    }

    /// ASCII fallback for no-unicode terminals. Must never be empty.
    #[rustfmt::skip]
    pub fn ascii_fallback(self) -> &'static str {
        match self {
            Self::Running => "*", Self::Active  => ">", Self::Queued  => "o", Self::Waiting => "...",
            Self::Passed  => "v", Self::Failed  => "x", Self::Blocked => "X", Self::Skipped => "/",
            Self::Proof   => "#", Self::Paused  => "||", Self::Grant  => "K", Self::Release => "R",
            Self::Warn    => "!", Self::Stale   => "~",
        }
    }

    /// Returns the unicode glyph if `unicode_ok`, otherwise the ASCII fallback.
    pub fn render(self, unicode_ok: bool) -> &'static str {
        if unicode_ok {
            self.unicode()
        } else {
            self.ascii_fallback()
        }
    }

    /// Full enum inventory for exhaustive iteration in tests and renderers.
    #[rustfmt::skip]
    pub const ALL: [Glyph; 14] = [
        Self::Running, Self::Active, Self::Queued, Self::Waiting,
        Self::Passed, Self::Failed, Self::Blocked, Self::Skipped,
        Self::Proof, Self::Paused, Self::Grant, Self::Release,
        Self::Warn, Self::Stale,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_has_ascii_fallback() {
        for g in Glyph::ALL {
            let a = g.ascii_fallback();
            assert!(!a.is_empty() && a.is_ascii(), "{g:?} fallback non-empty ASCII");
            assert!(!g.unicode().is_empty(), "{g:?} unicode non-empty");
        }
    }

    #[test]
    fn render_switches_on_unicode_capability() {
        assert_eq!(Glyph::Passed.render(true), "✓");
        assert_eq!(Glyph::Passed.render(false), "v");
        assert_eq!(Glyph::Failed.render(true), "✗");
        assert_eq!(Glyph::Failed.render(false), "x");
    }

    #[test]
    fn unicode_glyphs_match_inventory() {
        assert_eq!(Glyph::Running.unicode(), "●");
        assert_eq!(Glyph::Active.unicode(), "▶");
        assert_eq!(Glyph::Queued.unicode(), "○");
        assert_eq!(Glyph::Waiting.unicode(), "…");
        assert_eq!(Glyph::Proof.unicode(), "◆");
        assert_eq!(Glyph::Grant.unicode(), "⚿");
        assert_eq!(Glyph::Release.unicode(), "⬢");
    }
}
