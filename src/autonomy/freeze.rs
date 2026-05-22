//! Freeze-window enforcement (Wave 4 of the Evidence Gate rollout).
//!
//! A *freeze window* is a calendar interval during which the autonomous
//! delivery pipeline must not auto-merge changes above a configured risk
//! ceiling. Without this enforcement, no `sovereign_plus` profile is safe to
//! enable: a single misbehaving R0/R1/R2 change could ship straight to prod
//! during a code-freeze (end-of-year, on-call rotation, peak-traffic event).
//!
//! Design notes
//! ------------
//! * The named hard-stop `freeze_window_active` is already registered in
//!   `conditions.rs::ConditionRegistry::default()` as an externally-supplied
//!   condition. This module owns the *computation* of whether to inject it.
//! * Loading is strict YAML deserialisation against `vibegate.freeze.v1`.
//! * `RiskTier` does not derive `Ord`, so we compare against an explicit
//!   numeric rank (`risk_rank`). R0 = 0, R5 = 5. Higher rank = riskier.
//! * Window matching is half-open: `start <= now < end`.

use crate::autonomy::conditions::HardStop;
use crate::autonomy::types::RiskTier;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(test)]
#[path = "freeze_tests.rs"]
mod tests;

/// One contiguous calendar window during which automation is constrained.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreezeWindow {
    /// Stable, human-readable identifier (e.g. `holiday-2026-12-24`).
    pub id: String,
    /// Long-form display name shown in ledger / TUI / PR comments.
    pub name: String,
    /// Inclusive lower bound, UTC.
    pub start: DateTime<Utc>,
    /// Exclusive upper bound, UTC.
    pub end: DateTime<Utc>,
    /// Highest risk tier still allowed to auto-merge during this window.
    /// A change classified strictly above this tier triggers a hard stop.
    pub max_allowed_risk: RiskTier,
    /// Free-form rationale surfaced in the hard-stop reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// If true, a documented break-glass procedure can bypass the freeze
    /// (still audited; this flag is consulted by the orchestrator, not here).
    #[serde(default)]
    pub allow_break_glass: bool,
}

/// Strict-typed loader for `.jeryu/autonomy/policies/freeze.yml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FreezeWindows {
    pub schema: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub windows: Vec<FreezeWindow>,
}

impl FreezeWindows {
    /// Read and parse a `freeze.yml` from disk.
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| std::io::Error::new(e.kind(), format!("read {}: {e}", path.display())))?;
        Self::from_str_yaml(&s).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("parse {}: {e}", path.display()),
            )
        })
    }

    /// Parse a YAML string directly (used by tests and the bundle loader).
    pub fn from_str_yaml(s: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(s)
    }

    /// First window whose `[start, end)` interval contains `now`. Returns
    /// `None` when no window matches or when the policy is globally disabled.
    pub fn active_at(&self, now: DateTime<Utc>) -> Option<&FreezeWindow> {
        if !self.enabled {
            return None;
        }
        self.windows.iter().find(|w| w.start <= now && now < w.end)
    }

    /// If a window is active and `risk` exceeds its ceiling, return a hard
    /// stop the caller should inject as `freeze_window_active`. Otherwise
    /// `None`. Risk equal to or below the ceiling is permitted (the
    /// freeze acts as a *cap*, not a kill switch).
    pub fn check(&self, risk: RiskTier, now: DateTime<Utc>) -> Option<HardStop> {
        let w = self.active_at(now)?;
        if risk_rank(risk) <= risk_rank(w.max_allowed_risk) {
            return None;
        }
        let mut reason = format!(
            "freeze window '{}' active until {}; max allowed risk is {:?}, change is {:?}",
            w.name,
            w.end.to_rfc3339(),
            w.max_allowed_risk,
            risk,
        );
        if let Some(extra) = w.reason.as_ref() {
            reason.push_str(" (");
            reason.push_str(extra);
            reason.push(')');
        }
        Some(HardStop {
            name: "freeze_window_active".into(),
            reason,
            details: serde_json::json!({
                "window_id": w.id,
                "window_name": w.name,
                "end": w.end.to_rfc3339(),
                "max_allowed_risk": w.max_allowed_risk,
                "change_risk": risk,
                "allow_break_glass": w.allow_break_glass,
            }),
        })
    }
}

/// Numeric ordering for `RiskTier` (which does not derive `Ord`).
/// Higher = riskier. R0 → 0, R5 → 5.
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
