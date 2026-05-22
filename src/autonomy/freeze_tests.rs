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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".jeryu/autonomy/policies/freeze.yml");
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
