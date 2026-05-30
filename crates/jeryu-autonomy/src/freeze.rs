//! Freeze-window enforcement.
//!
//! A *freeze window* is a calendar interval during which the autonomous
//! delivery pipeline must not auto-merge changes above a configured risk
//! ceiling. The named hard-stop `freeze_window_active` is registered in
//! [`crate::conditions::ConditionRegistry`] as an externally-supplied
//! condition; this module owns the *computation* of whether to inject it.
//!
//! * `RiskTier` does not derive `Ord`, so comparison uses an explicit numeric
//!   rank (R0 = 0, R5 = 5). Higher rank = riskier.
//! * Window matching is half-open: `start <= now < end`.

use crate::conditions::HardStop;
use crate::types::RiskTier;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    /// Highest risk tier still allowed to auto-merge during this window. A
    /// change classified strictly above this tier triggers a hard stop.
    pub max_allowed_risk: RiskTier,
    /// Free-form rationale surfaced in the hard-stop reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// If true, a documented break-glass procedure can bypass the freeze
    /// (still audited; consulted by the orchestrator, not here).
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
    /// `None`. Risk equal to or below the ceiling is permitted (the freeze
    /// acts as a *cap*, not a kill switch).
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
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn windows_yaml() -> &'static str {
        r#"
schema: vibegate.freeze.v1
enabled: true
windows:
  - id: holiday-2026
    name: "Year-end freeze"
    start: "2026-12-24T00:00:00Z"
    end: "2027-01-02T00:00:00Z"
    max_allowed_risk: R1
    reason: "skeleton crew on-call"
    allow_break_glass: true
"#
    }

    fn at(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn parses_and_finds_active_window() {
        let f = FreezeWindows::from_str_yaml(windows_yaml()).unwrap();
        assert!(f.active_at(at(2026, 12, 25)).is_some());
        assert!(f.active_at(at(2026, 6, 1)).is_none());
    }

    #[test]
    fn check_caps_risk_above_ceiling() {
        let f = FreezeWindows::from_str_yaml(windows_yaml()).unwrap();
        // R2 > R1 ceiling → fires.
        let stop = f.check(RiskTier::R2, at(2026, 12, 25));
        assert!(stop.is_some());
        assert_eq!(stop.unwrap().name, "freeze_window_active");
        // R1 == ceiling → permitted (cap, not kill switch).
        assert!(f.check(RiskTier::R1, at(2026, 12, 25)).is_none());
        // R0 < ceiling → permitted.
        assert!(f.check(RiskTier::R0, at(2026, 12, 25)).is_none());
    }

    #[test]
    fn check_outside_window_never_fires() {
        let f = FreezeWindows::from_str_yaml(windows_yaml()).unwrap();
        assert!(f.check(RiskTier::R5, at(2026, 6, 1)).is_none());
    }

    #[test]
    fn disabled_policy_never_fires() {
        let mut f = FreezeWindows::from_str_yaml(windows_yaml()).unwrap();
        f.enabled = false;
        assert!(f.active_at(at(2026, 12, 25)).is_none());
        assert!(f.check(RiskTier::R5, at(2026, 12, 25)).is_none());
    }

    #[test]
    fn half_open_boundaries() {
        let f = FreezeWindows::from_str_yaml(windows_yaml()).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 12, 24, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2027, 1, 2, 0, 0, 0).unwrap();
        assert!(f.active_at(start).is_some(), "start is inclusive");
        assert!(f.active_at(end).is_none(), "end is exclusive");
    }
}
