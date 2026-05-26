//! Owner: Interactive TUI subsystem — fixture backend (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::`
//! Invariants:
//!   - All scenarios are pure functions returning typed `TuiReadModel` instances.
//!   - Timestamps are fixed (`Utc.with_ymd_and_hms(2026, 5, 26, ...)`) so two
//!     calls produce bytewise-identical JSON. The fixture backend is the
//!     screenshot/test substrate for the entire reset.
//!   - No I/O, no DB, no GitLab, no Docker, no Vault, no MCP, no clock reads.
//!   - No new third-party dependencies.

// Fixtures use `let mut m = TuiReadModel::default()` and patch fields
// individually; this is intentionally readable for scenario authors and
// keeps each scenario's struct shape close to the docs. Clippy's
// recommended struct-update syntax with `..Default::default()` is
// brittle here because `TuiReadModel` is non-exhaustive in spirit (new
// fields land via U04/U05 contracts).
#![allow(clippy::field_reassign_with_default)]

pub mod fixtures;

/// Deterministic UTC timestamp used by every fixture for byte-stable output.
/// Hour/minute/second are scenario-specific; date is locked to baseline day.
#[cfg(test)]
pub(crate) fn fixed_now() -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    chrono::Utc.with_ymd_and_hms(2026, 5, 26, 12, 0, 0).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::DataFreshness;
    use fixtures::{fresh, ts};

    #[test]
    fn fixed_now_is_deterministic_and_uses_baseline_date() {
        assert_eq!(fixed_now(), fixed_now());
        assert_eq!(format!("{}", fixed_now().format("%Y-%m-%d")), "2026-05-26");
    }

    #[test]
    fn fixture_ts_is_deterministic() {
        assert_eq!(ts(12, 0, 0), ts(12, 0, 0));
        assert_eq!(
            format!("{}", ts(9, 30, 0).format("%Y-%m-%d %H:%M:%S")),
            "2026-05-26 09:30:00"
        );
    }

    #[test]
    fn fresh_round_trips_and_marks_stale() {
        let f = fresh(10, 20, 30, 40, 50, false);
        let back: DataFreshness =
            serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(back.gitlab_ms, Some(10));
        assert!(!back.overall_stale);
        assert!(fresh(1, 1, 1, 1, 1, true).overall_stale);
    }
}
