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

/// Strict-typed loader for `.autonomy/policies/freeze.yml`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339 timestamp")
            .with_timezone(&Utc)
    }

    fn sample_window() -> FreezeWindow {
        FreezeWindow {
            id: "holiday-2026-12-24".into(),
            name: "End-of-year freeze 2026".into(),
            start: ts("2026-12-24T00:00:00Z"),
            end: ts("2027-01-02T00:00:00Z"),
            max_allowed_risk: RiskTier::R0,
            reason: Some("engineering on-call rotation reduced".into()),
            allow_break_glass: true,
        }
    }

    fn sample_policy(enabled: bool) -> FreezeWindows {
        FreezeWindows {
            schema: "vibegate.freeze.v1".into(),
            enabled,
            windows: vec![sample_window()],
        }
    }

    #[test]
    fn parse_minimal_yaml_round_trips() {
        let yaml = r#"
schema: vibegate.freeze.v1
enabled: true
windows:
  - id: holiday-2026-12-24
    name: "End-of-year freeze 2026"
    start: "2026-12-24T00:00:00Z"
    end:   "2027-01-02T00:00:00Z"
    max_allowed_risk: R0
    reason: "engineering on-call rotation reduced"
    allow_break_glass: true
"#;
        let parsed = FreezeWindows::from_str_yaml(yaml).expect("parses");
        assert_eq!(parsed.schema, "vibegate.freeze.v1");
        assert!(parsed.enabled);
        assert_eq!(parsed.windows.len(), 1);
        let w = &parsed.windows[0];
        assert_eq!(w.id, "holiday-2026-12-24");
        assert_eq!(w.name, "End-of-year freeze 2026");
        assert_eq!(w.max_allowed_risk, RiskTier::R0);
        assert!(w.allow_break_glass);
        assert_eq!(w.start, ts("2026-12-24T00:00:00Z"));
        assert_eq!(w.end, ts("2027-01-02T00:00:00Z"));

        // Round-trip through serialise→parse to confirm the schema is stable.
        let dumped = serde_yaml::to_string(&parsed).expect("serialises");
        let reparsed = FreezeWindows::from_str_yaml(&dumped).expect("reparses");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_real_freeze_yml_from_repo() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".autonomy/policies/freeze.yml");
        let loaded = FreezeWindows::from_path(&path)
            .expect("repo freeze.yml must parse with the FreezeWindows schema");
        assert_eq!(loaded.schema, "vibegate.freeze.v1");
        // The repo file must declare at least one window so operators can
        // see the shape; whether `enabled` is true depends on the repo state.
        assert!(
            !loaded.windows.is_empty(),
            "expected at least one example freeze window"
        );
    }

    #[test]
    fn active_at_returns_window_when_in_range() {
        let p = sample_policy(true);
        let now = Utc.with_ymd_and_hms(2026, 12, 28, 12, 0, 0).unwrap();
        let active = p.active_at(now).expect("window should be active");
        assert_eq!(active.id, "holiday-2026-12-24");
    }

    #[test]
    fn active_at_returns_none_before_window_starts() {
        let p = sample_policy(true);
        let now = Utc.with_ymd_and_hms(2026, 12, 23, 23, 59, 59).unwrap();
        assert!(p.active_at(now).is_none());
    }

    #[test]
    fn active_at_returns_none_after_window_ends() {
        let p = sample_policy(true);
        // `end` is exclusive — the instant 2027-01-02T00:00:00Z is *out*.
        let now = ts("2027-01-02T00:00:00Z");
        assert!(p.active_at(now).is_none());
        let later = Utc.with_ymd_and_hms(2027, 1, 5, 0, 0, 0).unwrap();
        assert!(p.active_at(later).is_none());
    }

    #[test]
    fn active_at_respects_enabled_flag() {
        let p = sample_policy(false);
        // Mid-window, but the policy is globally disabled → always None.
        let now = Utc.with_ymd_and_hms(2026, 12, 28, 12, 0, 0).unwrap();
        assert!(p.active_at(now).is_none());
    }

    #[test]
    fn check_returns_none_when_no_window_active() {
        let p = sample_policy(true);
        let now = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
        assert!(p.check(RiskTier::R5, now).is_none());
    }

    #[test]
    fn check_returns_hardstop_when_risk_above_max_during_window() {
        let p = sample_policy(true);
        let now = Utc.with_ymd_and_hms(2026, 12, 28, 12, 0, 0).unwrap();
        let stop = p
            .check(RiskTier::R2, now)
            .expect("R2 > R0 during freeze should fire");
        assert_eq!(stop.name, "freeze_window_active");
        assert_eq!(
            stop.details.get("window_id").and_then(|v| v.as_str()),
            Some("holiday-2026-12-24")
        );
        assert_eq!(
            stop.details
                .get("max_allowed_risk")
                .and_then(|v| v.as_str()),
            Some("R0")
        );
    }

    #[test]
    fn check_returns_none_when_risk_at_or_below_max_during_window() {
        let p = sample_policy(true);
        let now = Utc.with_ymd_and_hms(2026, 12, 28, 12, 0, 0).unwrap();
        // Exactly at the ceiling: allowed.
        assert!(p.check(RiskTier::R0, now).is_none());
        // Bump the ceiling to R2 to confirm "at-or-below" behaviour for both
        // an equal tier and a strictly-lower tier.
        let mut higher = p.clone();
        higher.windows[0].max_allowed_risk = RiskTier::R2;
        assert!(higher.check(RiskTier::R0, now).is_none());
        assert!(higher.check(RiskTier::R1, now).is_none());
        assert!(higher.check(RiskTier::R2, now).is_none());
        assert!(higher.check(RiskTier::R3, now).is_some());
    }

    #[test]
    fn check_hardstop_name_is_freeze_window_active() {
        let p = sample_policy(true);
        let now = Utc.with_ymd_and_hms(2026, 12, 28, 12, 0, 0).unwrap();
        let stop = p.check(RiskTier::R5, now).expect("must fire");
        // This name must match the entry registered in
        // ConditionRegistry::default() — otherwise the orchestrator's
        // external-hard-stop injection silently drops the freeze.
        assert_eq!(stop.name, "freeze_window_active");
    }

    #[test]
    fn risk_rank_orders_tiers_correctly() {
        // Belt-and-braces: if RiskTier ever grows a tier, this test forces
        // a visible revision to risk_rank() so freeze enforcement stays
        // correct.
        assert!(risk_rank(RiskTier::R0) < risk_rank(RiskTier::R1));
        assert!(risk_rank(RiskTier::R1) < risk_rank(RiskTier::R2));
        assert!(risk_rank(RiskTier::R2) < risk_rank(RiskTier::R3));
        assert!(risk_rank(RiskTier::R3) < risk_rank(RiskTier::R4));
        assert!(risk_rank(RiskTier::R4) < risk_rank(RiskTier::R5));
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// Overlapping windows where the EARLIER window is more permissive than
    /// the LATER. `active_at` deterministically returns the first declared
    /// match; therefore `check` evaluates against the first window's
    /// `max_allowed_risk`, not the strictest of the overlapping set.
    #[test]
    fn overlapping_windows_use_first_declared_for_check() {
        let policy = FreezeWindows {
            schema: "vibegate.freeze.v1".into(),
            enabled: true,
            windows: vec![
                FreezeWindow {
                    id: "broad".into(),
                    name: "Broad - permissive".into(),
                    start: ts("2026-12-20T00:00:00Z"),
                    end: ts("2027-01-10T00:00:00Z"),
                    // Permissive ceiling — R3 allowed.
                    max_allowed_risk: RiskTier::R3,
                    reason: None,
                    allow_break_glass: false,
                },
                FreezeWindow {
                    id: "narrow".into(),
                    name: "Narrow - strict".into(),
                    start: ts("2026-12-24T00:00:00Z"),
                    end: ts("2027-01-02T00:00:00Z"),
                    max_allowed_risk: RiskTier::R0,
                    reason: None,
                    allow_break_glass: false,
                },
            ],
        };
        // Inside both windows. R2 is below the broad ceiling (R3); since the
        // broad window is declared first it wins.
        let now = ts("2026-12-26T00:00:00Z");
        assert!(
            policy.check(RiskTier::R2, now).is_none(),
            "first-declared window's ceiling governs"
        );
        // R4 still exceeds the broad ceiling (R3) → must fire.
        assert!(policy.check(RiskTier::R4, now).is_some());
    }

    /// Exactly at the start boundary the window is active (start is
    /// inclusive). Exactly at the end the window is NOT active (end is
    /// exclusive). Half-open intervals are the contract.
    #[test]
    fn boundary_exactly_at_start_active_exactly_at_end_inactive() {
        let p = sample_policy(true);
        let start = ts("2026-12-24T00:00:00Z");
        let end = ts("2027-01-02T00:00:00Z");
        assert!(
            p.active_at(start).is_some(),
            "start is inclusive: window MUST be active"
        );
        assert!(
            p.active_at(end).is_none(),
            "end is exclusive: window MUST NOT be active"
        );
        // One nanosecond before end: still active.
        let just_before_end = end - chrono::Duration::nanoseconds(1);
        assert!(p.active_at(just_before_end).is_some());
    }

    /// `enabled: false` with a window strictly in the future must still
    /// silently produce no active window — disabled is a kill-switch
    /// regardless of when the windows fall.
    #[test]
    fn disabled_policy_with_future_window_never_activates() {
        let p = FreezeWindows {
            schema: "vibegate.freeze.v1".into(),
            enabled: false,
            windows: vec![FreezeWindow {
                id: "future".into(),
                name: "Future freeze".into(),
                start: ts("2099-01-01T00:00:00Z"),
                end: ts("2099-01-02T00:00:00Z"),
                max_allowed_risk: RiskTier::R0,
                reason: None,
                allow_break_glass: false,
            }],
        };
        // Inside the window
        let in_window = ts("2099-01-01T12:00:00Z");
        assert!(p.active_at(in_window).is_none());
        assert!(p.check(RiskTier::R5, in_window).is_none());
        // Before
        assert!(p.active_at(ts("2050-01-01T00:00:00Z")).is_none());
    }

    #[test]
    fn check_picks_first_overlapping_window_when_multiple_match() {
        // Defensive: if two windows overlap (operator error), we must still
        // pick one deterministically rather than panic or return any.
        let policy = FreezeWindows {
            schema: "vibegate.freeze.v1".into(),
            enabled: true,
            windows: vec![
                FreezeWindow {
                    id: "first".into(),
                    name: "First".into(),
                    start: ts("2026-12-24T00:00:00Z"),
                    end: ts("2027-01-02T00:00:00Z"),
                    max_allowed_risk: RiskTier::R1,
                    reason: None,
                    allow_break_glass: false,
                },
                FreezeWindow {
                    id: "second".into(),
                    name: "Second".into(),
                    start: ts("2026-12-26T00:00:00Z"),
                    end: ts("2026-12-30T00:00:00Z"),
                    max_allowed_risk: RiskTier::R0,
                    reason: None,
                    allow_break_glass: false,
                },
            ],
        };
        let now = ts("2026-12-27T00:00:00Z");
        let active = policy.active_at(now).expect("one must match");
        assert_eq!(active.id, "first", "first declared window wins on overlap");
    }
}
