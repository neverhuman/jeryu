//! Owner: Evidence Gate / autonomy control plane (Wave 5)
//! Proof: `cargo test -p jeryu --lib autonomy::profile`
//! Invariants:
//!   - `sovereign_plus` is THE on-switch for "100% to prod when configured".
//!     It MUST NOT be returned as the effective profile unless every Wave 1-4
//!     guardrail is wired and healthy. Any missing guardrail downgrades the
//!     effective profile to `sovereign` so the operator sees the gap loudly.
//!   - The validator never short-circuits: it runs every check on every call
//!     so the operator sees every gap in one report, not just the first.
//!   - The validator has no side effects: it only reads ledger state, the
//!     filesystem, and a caller-supplied shadow agreement rate. It does not
//!     mutate the kill bell, the freeze policy, the canary state, or the
//!     ledger. Pure read-only inspection.
//!
//! Brainstorm refs: `tips/fullauto/tip1.txt` (Law 7), `tip8.txt` (A6 profile),
//! `tip9.txt` (A6 sovereign autopilot). This is the Wave 5 surface that gates
//! every other Wave 1-4 component into a single bootable autonomy posture.

#[path = "profile_validate.rs"]
mod profile_validate;
pub use profile_validate::{ValidatorInputs, validate_sovereign_plus};

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;

/// The five (now six) named profiles defined in `.jeryu/autonomy/autonomy.yml`.
///
/// Order matches the YAML declaration order; `parse` is case-insensitive on
/// the `snake_case` name so CLI flags can accept either form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyProfile {
    ReportOnly,
    Supervised,
    AutonomousMerge,
    AutonomousRelease,
    Sovereign,
    SovereignPlus,
}

impl AutonomyProfile {
    /// Canonical `snake_case` name. Matches the YAML key exactly.
    pub fn name(&self) -> &'static str {
        match self {
            AutonomyProfile::ReportOnly => "report_only",
            AutonomyProfile::Supervised => "supervised",
            AutonomyProfile::AutonomousMerge => "autonomous_merge",
            AutonomyProfile::AutonomousRelease => "autonomous_release",
            AutonomyProfile::Sovereign => "sovereign",
            AutonomyProfile::SovereignPlus => "sovereign_plus",
        }
    }

    /// Parse a profile name. Case-insensitive. Returns `None` for unknown.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "report_only" => Some(AutonomyProfile::ReportOnly),
            "supervised" => Some(AutonomyProfile::Supervised),
            "autonomous_merge" => Some(AutonomyProfile::AutonomousMerge),
            "autonomous_release" => Some(AutonomyProfile::AutonomousRelease),
            "sovereign" => Some(AutonomyProfile::Sovereign),
            "sovereign_plus" => Some(AutonomyProfile::SovereignPlus),
            _ => None,
        }
    }
}

/// The set of preconditions `sovereign_plus` requires before it loads.
///
/// `Default` mirrors the `.jeryu/autonomy/autonomy.yml` `sovereign_plus` block:
/// every boolean is `true` and the shadow agreement floor is `0.95`. A caller
/// that wants to relax a guardrail (e.g. in a dev env) constructs the struct
/// explicitly rather than mutating defaults at runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct SovereignPlusGuardrails {
    pub require_nightwatch: bool,
    pub require_canary: bool,
    pub require_rollback_drill: bool,
    pub require_kill_bell_armed: bool,
    pub require_freeze_check: bool,
    pub require_shadow_agreement_min: f64,
}

impl Default for SovereignPlusGuardrails {
    fn default() -> Self {
        Self {
            require_nightwatch: true,
            require_canary: true,
            require_rollback_drill: true,
            require_kill_bell_armed: true,
            require_freeze_check: true,
            require_shadow_agreement_min: 0.95,
        }
    }
}

/// A single guardrail failure, with operator-actionable remediation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailFailure {
    pub guardrail: String,
    pub reason: String,
    pub remediation: String,
}

/// Result of running the startup validator. Always carries the full picture:
/// every guardrail that passed AND every guardrail that failed. The
/// `effective_profile` field is the profile the binary should actually run as:
/// `SovereignPlus` only when everything passed, else `Sovereign`.
#[derive(Debug, Clone)]
pub struct GuardrailReport {
    pub passed: Vec<String>,
    pub failed: Vec<GuardrailFailure>,
    pub effective_profile: AutonomyProfile,
}

impl GuardrailReport {
    /// `true` iff no guardrail failed (and therefore the effective profile is
    /// `SovereignPlus`).
    pub fn all_passed(&self) -> bool {
        self.failed.is_empty()
    }

    /// Operator-facing render. Emits a header, every passed guardrail (single
    /// line each), then every failed guardrail with reason + remediation. The
    /// final line states the effective profile. Designed for the startup log;
    /// stable enough to be grepped from CI without parsing JSON.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str("sovereign_plus startup validation\n");
        out.push_str("─────────────────────────────────\n");
        out.push_str(&format!(
            "passed: {}   failed: {}\n",
            self.passed.len(),
            self.failed.len()
        ));
        if !self.passed.is_empty() {
            out.push_str("\nPassed:\n");
            for p in &self.passed {
                out.push_str("  ✓ ");
                out.push_str(p);
                out.push('\n');
            }
        }
        if !self.failed.is_empty() {
            out.push_str("\nFailed:\n");
            for f in &self.failed {
                out.push_str("  ✗ ");
                out.push_str(&f.guardrail);
                out.push_str("\n      reason:      ");
                out.push_str(&f.reason);
                out.push_str("\n      remediation: ");
                out.push_str(&f.remediation);
                out.push('\n');
            }
        }
        out.push_str(&format!(
            "\neffective profile: {}\n",
            self.effective_profile.name()
        ));
        out
    }
}
