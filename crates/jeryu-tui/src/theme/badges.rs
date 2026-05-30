//! Semantic badges (freshness / proof / risk).
//!
//! Invariants: freshness/proof/risk labels are pinned verbatim; the named risk
//! aliases stay convertible into the canonical R0-R5 tiers.

use ratatui::style::{Modifier, Style};

use super::palette::Palette;

// ── Freshness badges ────────────────────────────────────────────────────

/// Freshness badge variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[rustfmt::skip]
pub enum FreshnessBadge {
    Live, Fresh, Expired, LastKnown, Inferred, Partial,
    SourceDown, Unknown, NoProof, Unverified, Poll, Fixture,
}

impl FreshnessBadge {
    /// Pinned display label.
    #[rustfmt::skip]
    pub fn label(self) -> &'static str {
        match self {
            Self::Live       => "LIVE",        Self::Fresh      => "FRESH",
            Self::Expired    => "EXPIRED",     Self::LastKnown  => "LAST KNOWN",
            Self::Inferred   => "INFERRED",    Self::Partial    => "PARTIAL",
            Self::SourceDown => "SOURCE DOWN", Self::Unknown    => "UNKNOWN",
            Self::NoProof    => "NO PROOF",    Self::Unverified => "UNVERIFIED",
            Self::Poll       => "[poll]",      Self::Fixture    => "FIXTURE",
        }
    }

    /// Style mapping: LIVE/FRESH use `running`, EXPIRED dims to amber,
    /// SOURCE DOWN uses `crit`, NO PROOF uses a `crit` outline.
    pub fn style(self, p: &Palette) -> Style {
        let base = Style::default();
        match self {
            Self::Live => base.fg(p.running).add_modifier(Modifier::BOLD),
            Self::Fresh => base.fg(p.running),
            Self::Expired | Self::Unverified => base.fg(p.warn).add_modifier(Modifier::DIM),
            Self::LastKnown | Self::Inferred | Self::Unknown => {
                base.fg(p.muted).add_modifier(Modifier::DIM)
            }
            Self::Partial => base.fg(p.warn),
            Self::SourceDown => base.fg(p.crit).add_modifier(Modifier::BOLD),
            Self::NoProof => base.fg(p.crit),
            Self::Poll => base.fg(p.muted),
            Self::Fixture => base.fg(p.agent).add_modifier(Modifier::ITALIC),
        }
    }
}

// ── Proof confidence badges ─────────────────────────────────────────────

/// Proof confidence variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[rustfmt::skip]
pub enum ProofBadge { Meas, Struct, Hist, Heur, Miss, Expired }

impl ProofBadge {
    #[rustfmt::skip]
    pub fn label(self) -> &'static str {
        match self {
            Self::Meas => "MEAS", Self::Struct => "STRUCT", Self::Hist => "HIST",
            Self::Heur => "HEUR", Self::Miss   => "MISS",   Self::Expired => "EXPIRED",
        }
    }

    pub fn style(self, p: &Palette) -> Style {
        match self {
            Self::Meas => Style::default().fg(p.ok).add_modifier(Modifier::BOLD),
            Self::Struct => Style::default().fg(p.ok),
            Self::Hist => Style::default().fg(p.cache),
            Self::Heur => Style::default().fg(p.warn),
            Self::Miss => Style::default().fg(p.crit),
            Self::Expired => Style::default().fg(p.muted).add_modifier(Modifier::DIM),
        }
    }
}

// ── Risk badges ─────────────────────────────────────────────────────────

/// Canonical R0-R5 risk tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[rustfmt::skip]
pub enum RiskBadge { R0, R1, R2, R3, R4, R5 }

/// Human-named risk aliases preserved by the migration table. These map onto
/// R0-R5 via `RiskBadge::from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[rustfmt::skip]
pub enum NamedRiskTier { ReadOnly, Low, High, Production }

impl RiskBadge {
    #[rustfmt::skip]
    pub fn label(self) -> &'static str {
        match self {
            Self::R0 => "R0", Self::R1 => "R1", Self::R2 => "R2",
            Self::R3 => "R3", Self::R4 => "R4", Self::R5 => "R5",
        }
    }

    /// Risk-tier glyph.
    #[rustfmt::skip]
    pub fn glyph(self) -> &'static str {
        match self {
            Self::R0 => "▸", Self::R1 => "◇", Self::R2 => "◉", // read / local / ci
            Self::R3 => "▣", Self::R4 => "▮", Self::R5 => "✦", // repo / release / destructive
        }
    }

    pub fn style(self, p: &Palette) -> Style {
        match self {
            Self::R0 => Style::default().fg(p.ok),
            Self::R1 => Style::default().fg(p.cache),
            Self::R2 => Style::default().fg(p.queued),
            Self::R3 => Style::default().fg(p.warn),
            Self::R4 => Style::default().fg(p.release).add_modifier(Modifier::BOLD),
            Self::R5 => Style::default().fg(p.crit).add_modifier(Modifier::BOLD),
        }
    }
}

impl From<NamedRiskTier> for RiskBadge {
    /// Migration map: `ReadOnly -> R0`, `Low -> R1`, `High -> R3`,
    /// `Production -> R4`. (Per-action upgrades to R2/R5 are caller-controlled.)
    fn from(tier: NamedRiskTier) -> Self {
        match tier {
            NamedRiskTier::ReadOnly => Self::R0,
            NamedRiskTier::Low => Self::R1,
            NamedRiskTier::High => Self::R3,
            NamedRiskTier::Production => Self::R4,
        }
    }
}

/// Map the read-model [`RiskTier`](jeryu_readmodel::RiskTier) to a render badge.
impl From<jeryu_readmodel::RiskTier> for RiskBadge {
    fn from(tier: jeryu_readmodel::RiskTier) -> Self {
        use jeryu_readmodel::RiskTier as T;
        match tier {
            T::R0 => Self::R0,
            T::R1 => Self::R1,
            T::R2 => Self::R2,
            T::R3 => Self::R3,
            T::R4 => Self::R4,
            T::R5 => Self::R5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_badge_labels_are_pinned() {
        assert_eq!(FreshnessBadge::Live.label(), "LIVE");
        assert_eq!(FreshnessBadge::Fresh.label(), "FRESH");
        assert_eq!(FreshnessBadge::Expired.label(), "EXPIRED");
        assert_eq!(FreshnessBadge::LastKnown.label(), "LAST KNOWN");
        assert_eq!(FreshnessBadge::Inferred.label(), "INFERRED");
        assert_eq!(FreshnessBadge::Partial.label(), "PARTIAL");
        assert_eq!(FreshnessBadge::SourceDown.label(), "SOURCE DOWN");
        assert_eq!(FreshnessBadge::Unknown.label(), "UNKNOWN");
        assert_eq!(FreshnessBadge::NoProof.label(), "NO PROOF");
        assert_eq!(FreshnessBadge::Unverified.label(), "UNVERIFIED");
        assert_eq!(FreshnessBadge::Poll.label(), "[poll]");
        assert_eq!(FreshnessBadge::Fixture.label(), "FIXTURE");
    }

    #[test]
    fn proof_badge_labels_are_pinned() {
        assert_eq!(ProofBadge::Meas.label(), "MEAS");
        assert_eq!(ProofBadge::Struct.label(), "STRUCT");
        assert_eq!(ProofBadge::Hist.label(), "HIST");
        assert_eq!(ProofBadge::Heur.label(), "HEUR");
        assert_eq!(ProofBadge::Miss.label(), "MISS");
        assert_eq!(ProofBadge::Expired.label(), "EXPIRED");
    }

    #[test]
    fn risk_badge_named_aliases() {
        assert_eq!(RiskBadge::from(NamedRiskTier::ReadOnly), RiskBadge::R0);
        assert_eq!(RiskBadge::from(NamedRiskTier::Low), RiskBadge::R1);
        assert_eq!(RiskBadge::from(NamedRiskTier::High), RiskBadge::R3);
        assert_eq!(RiskBadge::from(NamedRiskTier::Production), RiskBadge::R4);
    }

    #[test]
    fn read_model_risk_tier_maps_to_badge() {
        assert_eq!(
            RiskBadge::from(jeryu_readmodel::RiskTier::R2),
            RiskBadge::R2
        );
        assert_eq!(
            RiskBadge::from(jeryu_readmodel::RiskTier::R5),
            RiskBadge::R5
        );
    }

    #[test]
    fn every_badge_styles_against_palette() {
        let p = Palette::dark();
        let fresh = [
            FreshnessBadge::Live,
            FreshnessBadge::Fresh,
            FreshnessBadge::Expired,
            FreshnessBadge::LastKnown,
            FreshnessBadge::Inferred,
            FreshnessBadge::Partial,
            FreshnessBadge::SourceDown,
            FreshnessBadge::Unknown,
            FreshnessBadge::NoProof,
            FreshnessBadge::Unverified,
            FreshnessBadge::Poll,
            FreshnessBadge::Fixture,
        ];
        for b in fresh {
            let _ = b.style(&p);
        }
        let proof = [
            ProofBadge::Meas,
            ProofBadge::Struct,
            ProofBadge::Hist,
            ProofBadge::Heur,
            ProofBadge::Miss,
            ProofBadge::Expired,
        ];
        for b in proof {
            let _ = b.style(&p);
        }
        let risk = [
            RiskBadge::R0,
            RiskBadge::R1,
            RiskBadge::R2,
            RiskBadge::R3,
            RiskBadge::R4,
            RiskBadge::R5,
        ];
        for b in risk {
            let _ = b.style(&p);
            assert!(!b.glyph().is_empty() && !b.label().is_empty());
        }
    }
}
