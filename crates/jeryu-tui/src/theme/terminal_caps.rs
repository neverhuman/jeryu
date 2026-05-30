//! Terminal capability detection (ported from the source TUI theme).
//!
//! Invariants: env reads happen only inside `detect()`; the pure constructor
//! `from_env_values` takes env strings as arguments so tests stay deterministic.

/// Capability flags for the current terminal. Higher levels imply lower levels:
/// `supports_truecolor` ⇒ `supports_256` ⇒ `supports_color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCaps {
    pub supports_truecolor: bool,
    pub supports_256: bool,
    pub supports_color: bool,
    pub supports_unicode: bool,
}

impl TerminalCaps {
    /// Conservative ASCII / no-color fallback used when nothing is detectable.
    #[rustfmt::skip]
    pub const fn ascii_no_color() -> Self {
        Self { supports_truecolor: false, supports_256: false, supports_color: false, supports_unicode: false }
    }

    /// Full truecolor + unicode (used by snapshot tests and modern terminals).
    #[rustfmt::skip]
    pub const fn truecolor_unicode() -> Self {
        Self { supports_truecolor: true, supports_256: true, supports_color: true, supports_unicode: true }
    }

    /// Detect capabilities from process env. The only function here that
    /// performs `std::env` reads.
    #[rustfmt::skip]
    pub fn detect() -> Self {
        let ct = std::env::var("COLORTERM").ok();
        let t = std::env::var("TERM").ok();
        let nc = std::env::var("NO_COLOR").ok();
        let lang = std::env::var("LANG").ok();
        let lc = std::env::var("LC_ALL").ok();
        Self::from_env_values(ct.as_deref(), t.as_deref(), nc.as_deref(), lang.as_deref(), lc.as_deref())
    }

    /// Pure constructor from already-extracted env strings.
    #[rustfmt::skip]
    pub fn from_env_values(
        colorterm: Option<&str>, term: Option<&str>, no_color: Option<&str>,
        lang: Option<&str>, lc_all: Option<&str>,
    ) -> Self {
        // NO_COLOR (https://no-color.org/) wins — disables color when non-empty.
        let no_color_set = no_color.map(|v| !v.is_empty()).unwrap_or(false);
        let supports_truecolor = !no_color_set
            && colorterm.map(|v| v.eq_ignore_ascii_case("truecolor") || v.eq_ignore_ascii_case("24bit")).unwrap_or(false);
        let supports_256 = !no_color_set
            && (supports_truecolor || term.map(|t| t.contains("256") || t.contains("direct")).unwrap_or(false));
        let supports_color = !no_color_set
            && (supports_256 || term.map(|t| t != "dumb" && (
                t.contains("color") || t.starts_with("xterm") || t.starts_with("screen")
                || t.starts_with("tmux") || t.starts_with("rxvt") || t.starts_with("vt")
            )).unwrap_or(false));
        let locale = lc_all.or(lang).unwrap_or("").to_ascii_uppercase();
        let supports_unicode = locale.contains("UTF-8") || locale.contains("UTF8");
        Self { supports_truecolor, supports_256, supports_color, supports_unicode }
    }
}

impl Default for TerminalCaps {
    fn default() -> Self {
        Self::ascii_no_color()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_env_is_conservative() {
        let c = TerminalCaps::from_env_values(None, None, None, None, None);
        assert!(!c.supports_truecolor && !c.supports_256 && !c.supports_color && !c.supports_unicode);
    }

    #[test]
    fn truecolor_implies_256_and_color() {
        let c = TerminalCaps::from_env_values(Some("truecolor"), Some("xterm"), None, None, None);
        assert!(c.supports_truecolor && c.supports_256 && c.supports_color);
    }

    #[test]
    fn colorterm_24bit_accepted() {
        let c = TerminalCaps::from_env_values(Some("24BIT"), Some("xterm"), None, None, None);
        assert!(c.supports_truecolor);
    }

    #[test]
    fn term_256color_enables_256_not_truecolor() {
        let c = TerminalCaps::from_env_values(None, Some("xterm-256color"), None, None, None);
        assert!(!c.supports_truecolor && c.supports_256 && c.supports_color);
    }

    #[test]
    fn term_xterm_enables_basic_color_only() {
        let c = TerminalCaps::from_env_values(None, Some("xterm"), None, None, None);
        assert!(!c.supports_truecolor && !c.supports_256 && c.supports_color);
    }

    #[test]
    fn term_dumb_disables_heuristic_color() {
        let c = TerminalCaps::from_env_values(None, Some("dumb"), None, None, None);
        assert!(!c.supports_color);
    }

    #[test]
    #[rustfmt::skip]
    fn no_color_overrides_everything() {
        let c = TerminalCaps::from_env_values(Some("truecolor"), Some("xterm-256color"), Some("1"), None, None);
        assert!(!c.supports_truecolor && !c.supports_256 && !c.supports_color);
    }

    #[test]
    fn utf8_locale_enables_unicode() {
        let lang = TerminalCaps::from_env_values(None, None, None, Some("en_US.UTF-8"), None);
        let lc = TerminalCaps::from_env_values(None, None, None, Some("C"), Some("en_US.UTF-8"));
        assert!(lang.supports_unicode && lc.supports_unicode);
    }
}
