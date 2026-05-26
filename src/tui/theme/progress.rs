//! Owner: Interactive TUI subsystem — progress bar styling (U12).
//! Proof: `cargo nextest run -p jeryu --lib tui::theme::progress`.
//! Invariants: progress values are clamped to `[0.0, 1.0]`; bar color is
//! selected by the proof confidence variant so visual quality matches the
//! evidence quality.

use ratatui::style::{Color, Modifier, Style};

use super::badges::ProofBadge;
use super::palette::Palette;

/// Confidence enum aligned with `ProofBadge` — duplicated here so callers
/// constructing a `ProgressBar` from raw confidence data do not need to
/// import the badge type when they just want a bar color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Meas,
    Struct,
    Hist,
    Heur,
    Miss,
    Stale,
}

impl Confidence {
    pub fn as_badge(self) -> ProofBadge {
        match self {
            Self::Meas => ProofBadge::Meas,
            Self::Struct => ProofBadge::Struct,
            Self::Hist => ProofBadge::Hist,
            Self::Heur => ProofBadge::Heur,
            Self::Miss => ProofBadge::Miss,
            Self::Stale => ProofBadge::Stale,
        }
    }
}

/// Simple progress bar view-model. `value` is clamped to `[0.0, 1.0]` at
/// construction time; `kind` selects the bar's foreground color so a
/// heuristic bar reads visually different from a measured bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressBar {
    pub value: f64,
    pub kind: Confidence,
}

impl ProgressBar {
    /// Construct a bar with `value` clamped into `[0.0, 1.0]`.
    pub fn new(value: f64, kind: Confidence) -> Self {
        let clamped = if value.is_nan() {
            0.0
        } else {
            value.clamp(0.0, 1.0)
        };
        Self {
            value: clamped,
            kind,
        }
    }

    /// Bar color derived from confidence kind.
    pub fn color(self, p: &Palette) -> Color {
        match self.kind {
            Confidence::Meas | Confidence::Struct => p.ok,
            Confidence::Hist => p.cache,
            Confidence::Heur => p.warn,
            Confidence::Miss => p.crit,
            Confidence::Stale => p.stale,
        }
    }

    /// Full Style for the filled portion of the bar.
    pub fn style(self, p: &Palette) -> Style {
        let mut s = Style::default().fg(self.color(p));
        if matches!(self.kind, Confidence::Stale) {
            s = s.add_modifier(Modifier::DIM);
        }
        s
    }

    /// Render the bar as a fixed-width string of filled/empty cells.
    /// Pure function — no terminal queries. Caller passes the available
    /// width in cells.
    pub fn render(self, width: u16) -> String {
        if width == 0 {
            return String::new();
        }
        let total = width as usize;
        let filled = (self.value * width as f64).round() as usize;
        let filled = filled.min(total);
        let mut s = String::with_capacity(total);
        for _ in 0..filled {
            s.push('█');
        }
        for _ in filled..total {
            s.push('░');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values outside `[0.0, 1.0]` are clamped. NaN becomes 0.0.
    #[test]
    fn progress_bar_value_clamps_0_to_1() {
        assert_eq!(ProgressBar::new(-0.5, Confidence::Meas).value, 0.0);
        assert_eq!(ProgressBar::new(0.0, Confidence::Meas).value, 0.0);
        assert_eq!(ProgressBar::new(0.5, Confidence::Meas).value, 0.5);
        assert_eq!(ProgressBar::new(1.0, Confidence::Meas).value, 1.0);
        assert_eq!(ProgressBar::new(1.5, Confidence::Meas).value, 1.0);
        assert_eq!(ProgressBar::new(f64::NAN, Confidence::Meas).value, 0.0);
    }

    /// Color follows confidence: measured/structural use `ok`, heuristic
    /// uses `warn`, missing uses `crit`.
    #[test]
    fn color_follows_confidence() {
        let p = Palette::dark();
        assert_eq!(ProgressBar::new(0.5, Confidence::Meas).color(&p), p.ok);
        assert_eq!(ProgressBar::new(0.5, Confidence::Struct).color(&p), p.ok);
        assert_eq!(ProgressBar::new(0.5, Confidence::Hist).color(&p), p.cache);
        assert_eq!(ProgressBar::new(0.5, Confidence::Heur).color(&p), p.warn);
        assert_eq!(ProgressBar::new(0.5, Confidence::Miss).color(&p), p.crit);
        assert_eq!(ProgressBar::new(0.5, Confidence::Stale).color(&p), p.stale);
    }

    /// Confidence ↔ ProofBadge mapping is total.
    #[test]
    fn confidence_maps_to_proof_badge() {
        assert_eq!(Confidence::Meas.as_badge(), ProofBadge::Meas);
        assert_eq!(Confidence::Struct.as_badge(), ProofBadge::Struct);
        assert_eq!(Confidence::Hist.as_badge(), ProofBadge::Hist);
        assert_eq!(Confidence::Heur.as_badge(), ProofBadge::Heur);
        assert_eq!(Confidence::Miss.as_badge(), ProofBadge::Miss);
        assert_eq!(Confidence::Stale.as_badge(), ProofBadge::Stale);
    }

    /// `render(0)` returns empty; otherwise the string has exactly `width`
    /// graphemes (`█` or `░`), with filled-cell count matching `value`.
    #[test]
    fn render_produces_fixed_width_bars() {
        let zero = ProgressBar::new(0.0, Confidence::Meas);
        let half = ProgressBar::new(0.5, Confidence::Meas);
        let full = ProgressBar::new(1.0, Confidence::Meas);
        assert_eq!(zero.render(0), "");
        assert_eq!(zero.render(10).chars().count(), 10);
        assert_eq!(half.render(10).chars().count(), 10);
        assert_eq!(full.render(10), "██████████");
        assert_eq!(zero.render(10), "░░░░░░░░░░");
        // Half-and-half at width 10: 5 filled, 5 empty.
        assert_eq!(half.render(10), "█████░░░░░");
    }

    /// Stale bars carry the DIM modifier; non-stale bars do not.
    #[test]
    fn stale_bar_is_dim() {
        let p = Palette::dark();
        let stale = ProgressBar::new(0.5, Confidence::Stale).style(&p);
        let fresh = ProgressBar::new(0.5, Confidence::Meas).style(&p);
        assert!(stale.add_modifier.contains(Modifier::DIM));
        assert!(!fresh.add_modifier.contains(Modifier::DIM));
    }
}
