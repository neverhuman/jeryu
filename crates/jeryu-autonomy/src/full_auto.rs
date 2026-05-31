//! FULL-AUTO profile — the dogfood auto-merge brain.
//!
//! The Evidence Gate's *default* posture is conservative: today
//! [`RiskTier::auto_merge_eligible`](crate::types::RiskTier::auto_merge_eligible)
//! is `R0|R1|R2` and [`RiskTier::human_required`](crate::types::RiskTier::human_required)
//! is `R3|R4|R5`, and the canonical [`PolicyBundle`] fixtures land
//! [`GateDecision::RequireHuman`] at `R3`/`R4` via the per-tier
//! `quorum.human_required` flag.
//!
//! An owner who wants their repo to merge fully autonomously installs a
//! **FULL-AUTO profile**: a small, signed-by-policy descriptor that *raises the
//! human-required floor to R5 only*. Under a loaded full-auto profile, `R0..=R4`
//! become AllowMerge-eligible; `R5` stays fail-closed (human required).
//!
//! ## What full-auto changes — and what it categorically does NOT
//!
//! Full-auto is **policy-authoritative for the human-required floor only**. It
//! is layered so it can NEVER weaken the safety floor:
//!
//! * **R5 stays fail-closed.** No profile can make `R5` AllowMerge-eligible;
//!   [`FullAutoProfile::validate`] rejects any descriptor that tries.
//! * **Hard stops still veto.** A profile never touches the conditions registry,
//!   the `approvals.hard_stops` list, or the judge's hard-stop walk, so any hit
//!   still produces [`GateDecision::Reject`] (veto > approval). [`resolve`]
//!   re-asserts this: a `Reject` input is returned unchanged at every tier.
//! * **Kill-bell still downgrades.** Full-auto operates on the *pre*-bell
//!   verdict; the [`KillBell`](crate::kill_bell::KillBell) hot-path downgrade to
//!   `RequireHuman` runs after and is unaffected.
//! * **Freeze + protect_main intact.** Freeze windows inject a
//!   `freeze_window_active` hard stop (→ `Reject`) and protected paths classify
//!   to `R4`/`R5` upstream of the judge; full-auto changes neither.
//!
//! The mechanism is deliberately small: [`FullAutoProfile::apply`] returns a
//! *derived* [`PolicyBundle`] whose `R0..=R4` quorum entries have
//! `human_required = false` (and `fail_closed_without_human = false`) while the
//! `R5` entry keeps `human_required = true` / `fail_closed = true`. The existing
//! *pure* [`judge`](crate::judge::judge) then fuses against the derived bundle
//! exactly as before — no judge, quorum, or [`RiskTier`] change is required, and
//! every agent-reviewer quorum requirement (roles, counts, distinct identities,
//! no-self-approval) is preserved verbatim.

use crate::policy_yaml::PolicyBundle;
use crate::types::{GateDecision, RiskTier};
use serde::{Deserialize, Serialize};

/// Canonical schema tag for a full-auto profile descriptor.
pub const FULL_AUTO_SCHEMA: &str = "vibegate.full_auto_profile.v1";

/// The highest tier full-auto may make AllowMerge-eligible. `R5` is *always*
/// human-required — this is the fail-closed safety floor and cannot be lowered.
pub const FULL_AUTO_MAX_AUTO_TIER: RiskTier = RiskTier::R4;

/// Errors raised while loading / validating a full-auto profile descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullAutoError {
    /// The descriptor YAML failed to parse or carried an unknown field.
    Parse(String),
    /// The descriptor's `schema` tag was not [`FULL_AUTO_SCHEMA`].
    SchemaMismatch { got: String },
    /// The descriptor tried to push the auto ceiling past `R4` (i.e. tried to
    /// auto-merge `R5`). Rejected: `R5` is fail-closed.
    R5NotFailClosed,
    /// The descriptor's declared `max_auto_tier` was not a recognized tier.
    UnknownTier { got: String },
    /// The wrapped [`PolicyBundle`] is missing a required `R5` quorum entry, so
    /// the fail-closed floor cannot be guaranteed.
    MissingR5Floor,
}

impl std::fmt::Display for FullAutoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FullAutoError::Parse(e) => write!(f, "full-auto profile parse error: {e}"),
            FullAutoError::SchemaMismatch { got } => {
                write!(
                    f,
                    "full-auto profile schema mismatch: expected {FULL_AUTO_SCHEMA}, got {got}"
                )
            }
            FullAutoError::R5NotFailClosed => write!(
                f,
                "full-auto profile may not auto-merge R5: R5 is fail-closed (human required)"
            ),
            FullAutoError::UnknownTier { got } => {
                write!(f, "full-auto profile declares unknown max_auto_tier: {got}")
            }
            FullAutoError::MissingR5Floor => write!(
                f,
                "policy bundle has no R5 quorum entry; the fail-closed floor cannot be guaranteed"
            ),
        }
    }
}

impl std::error::Error for FullAutoError {}

/// Strict-typed, on-disk full-auto profile descriptor
/// (`.jeryu/autonomy/profiles/full-auto.yml`).
///
/// `#[serde(deny_unknown_fields)]` so a typo or an attempt to smuggle in an
/// out-of-band knob fails closed at load time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullAutoDescriptor {
    /// Must be [`FULL_AUTO_SCHEMA`].
    pub schema: String,
    /// Whether full-auto is engaged. A descriptor with `enabled: false` parses
    /// and validates, but [`FullAutoProfile::apply`] is a no-op (the bundle is
    /// returned unchanged), so the conservative default floor stays in force.
    #[serde(default)]
    pub enabled: bool,
    /// Highest tier the owner authorizes for unattended auto-merge. Defaults to
    /// `R4` (the maximum). It is an error to declare `R5` here — `R5` is always
    /// human-required.
    #[serde(default = "default_max_auto_tier")]
    pub max_auto_tier: RiskTier,
    /// Free-form owner rationale, surfaced in ledger / PR comments. Not
    /// load-bearing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn default_max_auto_tier() -> RiskTier {
    FULL_AUTO_MAX_AUTO_TIER
}

impl FullAutoDescriptor {
    /// Parse a descriptor from a YAML string. Unknown fields fail closed.
    pub fn from_yaml(s: &str) -> Result<Self, FullAutoError> {
        serde_yaml::from_str(s).map_err(|e| FullAutoError::Parse(e.to_string()))
    }
}

/// A loaded, validated FULL-AUTO profile bound to a concrete [`PolicyBundle`].
///
/// Construct via [`FullAutoProfile::new`] (from a bundle) or
/// [`FullAutoProfile::from_yaml`] (descriptor + bundle). Both run
/// [`FullAutoProfile::validate`], so an existing `FullAutoProfile` value is a
/// proof that the descriptor is well-formed and the bundle carries an `R5`
/// floor.
#[derive(Debug, Clone)]
pub struct FullAutoProfile {
    descriptor: FullAutoDescriptor,
    bundle: PolicyBundle,
}

impl FullAutoProfile {
    /// Build the canonical full-auto profile over `bundle`: full-auto enabled,
    /// auto ceiling at `R4`. Validates before returning.
    pub fn new(bundle: PolicyBundle) -> Result<Self, FullAutoError> {
        Self::with_descriptor(
            FullAutoDescriptor {
                schema: FULL_AUTO_SCHEMA.to_string(),
                enabled: true,
                max_auto_tier: FULL_AUTO_MAX_AUTO_TIER,
                reason: None,
            },
            bundle,
        )
    }

    /// Build a profile from an explicit descriptor + bundle. Validates before
    /// returning.
    pub fn with_descriptor(
        descriptor: FullAutoDescriptor,
        bundle: PolicyBundle,
    ) -> Result<Self, FullAutoError> {
        let p = Self { descriptor, bundle };
        p.validate()?;
        Ok(p)
    }

    /// Load a profile from a descriptor YAML string + an already-loaded
    /// [`PolicyBundle`]. Validates before returning.
    pub fn from_yaml(descriptor_yaml: &str, bundle: PolicyBundle) -> Result<Self, FullAutoError> {
        let descriptor = FullAutoDescriptor::from_yaml(descriptor_yaml)?;
        Self::with_descriptor(descriptor, bundle)
    }

    /// Validate a loaded profile:
    ///   1. schema tag is exactly [`FULL_AUTO_SCHEMA`];
    ///   2. `max_auto_tier` never reaches `R5` (R5 stays fail-closed);
    ///   3. the wrapped bundle declares an `R5` quorum entry that is
    ///      human-required (the floor exists and is intact).
    pub fn validate(&self) -> Result<(), FullAutoError> {
        if self.descriptor.schema != FULL_AUTO_SCHEMA {
            return Err(FullAutoError::SchemaMismatch {
                got: self.descriptor.schema.clone(),
            });
        }
        // The auto ceiling may never include R5.
        if matches!(self.descriptor.max_auto_tier, RiskTier::R5) {
            return Err(FullAutoError::R5NotFailClosed);
        }
        // The R5 fail-closed floor must exist in the bundle and require a human.
        match self.bundle.quorum_for(RiskTier::R5) {
            Some(q) if q.human_required => Ok(()),
            _ => Err(FullAutoError::MissingR5Floor),
        }
    }

    /// `true` when this profile is enabled (the canonical [`new`](Self::new)
    /// profile always is).
    pub fn is_enabled(&self) -> bool {
        self.descriptor.enabled
    }

    /// The owner-authorized auto ceiling.
    pub fn max_auto_tier(&self) -> RiskTier {
        self.descriptor.max_auto_tier
    }

    /// The (unmodified) source bundle this profile wraps.
    pub fn source_bundle(&self) -> &PolicyBundle {
        &self.bundle
    }

    /// Borrow the descriptor.
    pub fn descriptor(&self) -> &FullAutoDescriptor {
        &self.descriptor
    }

    /// Policy-authoritative tier check: is `tier` AllowMerge-eligible under this
    /// profile? Under an enabled full-auto profile this is `R0..=max_auto_tier`
    /// (so `R0..=R4`, never `R5`). Under a disabled profile this falls back to
    /// the crate-default [`RiskTier::auto_merge_eligible`] (`R0|R1|R2`).
    pub fn tier_auto_merge_eligible(&self, tier: RiskTier) -> bool {
        if !self.descriptor.enabled {
            return tier.auto_merge_eligible();
        }
        // R5 is always human-required, regardless of the declared ceiling.
        if matches!(tier, RiskTier::R5) {
            return false;
        }
        risk_rank(tier) <= risk_rank(self.descriptor.max_auto_tier)
    }

    /// Derive the [`PolicyBundle`] the judge should fuse against.
    ///
    /// For an **enabled** profile: every quorum entry for a tier at or below the
    /// auto ceiling has its `human_required` / `fail_closed_without_human` flags
    /// cleared, so the pure judge can land [`GateDecision::AllowMerge`] once the
    /// (unchanged) agent-reviewer quorum is met. The `R5` entry is left exactly
    /// as authored — `human_required` / `fail_closed` stay set, so `R5` still
    /// lands `RequireHuman`.
    ///
    /// For a **disabled** profile this is a verbatim clone of the source bundle.
    ///
    /// This never weakens hard stops, freeze, or protect_main: those live
    /// outside the per-tier quorum `human_required` flag and are untouched.
    pub fn apply(&self) -> PolicyBundle {
        let mut bundle = self.bundle.clone();
        if !self.descriptor.enabled {
            return bundle;
        }
        for (tier, entry) in bundle.approvals.quorum.iter_mut() {
            // The R5 floor is sacrosanct; never relax it.
            if matches!(tier, RiskTier::R5) {
                continue;
            }
            if risk_rank(*tier) <= risk_rank(self.descriptor.max_auto_tier) {
                entry.human_required = false;
                entry.fail_closed_without_human = false;
            }
        }
        bundle
    }

    /// Apply the full-auto safety-floor sandwich to a judge-produced decision.
    ///
    /// This is the *post*-judge re-assertion of the invariants, independent of
    /// the derived bundle, so the floor holds even if a caller fuses against the
    /// raw bundle:
    ///   * `Reject` is returned unchanged at **every** tier (veto > approval).
    ///   * At `R5` (or any tier above the auto ceiling), an `AllowMerge` is
    ///     downgraded to `RequireHuman` (fail-closed).
    ///   * `RequireHuman` is returned unchanged.
    ///   * Otherwise `AllowMerge` passes through.
    pub fn resolve(&self, tier: RiskTier, decision: GateDecision) -> GateDecision {
        match decision {
            // Veto beats approval, always — at every tier.
            GateDecision::Reject => GateDecision::Reject,
            GateDecision::RequireHuman => GateDecision::RequireHuman,
            GateDecision::AllowMerge => {
                if self.tier_auto_merge_eligible(tier) {
                    GateDecision::AllowMerge
                } else {
                    // R5 / above-ceiling: fail closed.
                    GateDecision::RequireHuman
                }
            }
        }
    }
}

/// Numeric ordering for `RiskTier` (which does not derive `Ord`). Mirrors
/// [`crate::freeze`]'s private ranking; higher rank = riskier.
fn risk_rank(t: RiskTier) -> u8 {
    match t {
        RiskTier::R0 => 0,
        RiskTier::R1 => 1,
        RiskTier::R2 => 2,
        RiskTier::R3 => 3,
        RiskTier::R4 => 4,
        RiskTier::R5 => 5,
    }
}

#[cfg(test)]
mod tests;
