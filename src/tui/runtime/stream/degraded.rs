//! Owner: TUI runtime - DegradedTimeline (typed UI signal for [poll] /
//! LAST KNOWN / SOURCE DOWN badges).
//! Proof: `cargo nextest run -p jeryu --lib tui::runtime::stream::degraded`
//! Invariants:
//!   - `DegradedReason` is exhaustive and serde-stable so capture/replay
//!     tests can assert exact transitions (plan §29).
//!   - `DegradedTimeline::current()` returns the most-recent reason
//!     observed in the timeline (or `None` if the stream is fully
//!     healthy). UI reads this when picking the header badge.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Why the live stream is currently degraded. Stored in
/// `DegradedTimeline` and rendered by the UI as a badge + tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedReason {
    /// SSE failed, falling back to HTTP polling.
    SseUnavailable,
    /// HTTP polling failed too, last known state is shown.
    PollFailed,
    /// Cursor lag exceeded the freshness window.
    CursorLag,
    /// Backend reported its own degraded source (DB/GitLab/Docker down).
    BackendDegraded,
    /// Schema mismatch — UI is showing partial data.
    SchemaMismatch,
    /// Fixture transport active (demo / screenshots).
    FixtureActive,
}

impl DegradedReason {
    /// Short header badge label.
    pub fn badge(self) -> &'static str {
        match self {
            Self::SseUnavailable => "[poll]",
            Self::PollFailed => "SOURCE DOWN",
            Self::CursorLag => "STALE",
            Self::BackendDegraded => "DEGRADED",
            Self::SchemaMismatch => "PARTIAL",
            Self::FixtureActive => "FIXTURE",
        }
    }

    /// One-line description for the Source Doctor tooltip.
    pub fn description(self) -> &'static str {
        match self {
            Self::SseUnavailable => "SSE failed; HTTP polling is active.",
            Self::PollFailed => "HTTP polling failed; showing last known state.",
            Self::CursorLag => "Event cursor is behind the freshness window.",
            Self::BackendDegraded => "Backend reported a degraded source.",
            Self::SchemaMismatch => "Schema mismatch; partial projection only.",
            Self::FixtureActive => "Fixture transport active (demo / capture).",
        }
    }
}

/// One observation in the degraded timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DegradedEntry {
    pub reason: DegradedReason,
    pub at: DateTime<Utc>,
    pub note: Option<String>,
}

/// Rolling history of degraded states. UI reads this to render the
/// Source Doctor entry and the header badge.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DegradedTimeline {
    pub entries: Vec<DegradedEntry>,
}

impl DegradedTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a new degraded observation.
    pub fn push(&mut self, reason: DegradedReason, note: Option<String>) {
        self.entries.push(DegradedEntry {
            reason,
            at: Utc::now(),
            note,
        });
    }

    /// Most recent reason observed; `None` if timeline is empty.
    pub fn current(&self) -> Option<DegradedReason> {
        self.entries.last().map(|e| e.reason)
    }

    /// Clear the timeline (used when SSE recovers).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// True iff the timeline currently records any degraded state.
    pub fn is_degraded(&self) -> bool {
        !self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degraded_reason_badges_match_plan_language() {
        assert_eq!(DegradedReason::SseUnavailable.badge(), "[poll]");
        assert_eq!(DegradedReason::PollFailed.badge(), "SOURCE DOWN");
        assert_eq!(DegradedReason::FixtureActive.badge(), "FIXTURE");
    }

    #[test]
    fn timeline_records_reasons_in_order() {
        let mut t = DegradedTimeline::new();
        assert!(!t.is_degraded());
        t.push(DegradedReason::SseUnavailable, None);
        t.push(
            DegradedReason::PollFailed,
            Some("connection refused".into()),
        );
        assert!(t.is_degraded());
        assert_eq!(t.current(), Some(DegradedReason::PollFailed));
        assert_eq!(t.entries.len(), 2);
    }

    #[test]
    fn timeline_clear_resets_state() {
        let mut t = DegradedTimeline::new();
        t.push(DegradedReason::CursorLag, None);
        t.clear();
        assert!(!t.is_degraded());
        assert_eq!(t.current(), None);
    }

    #[test]
    fn degraded_reason_serde_roundtrip() {
        let json = serde_json::to_string(&DegradedReason::SseUnavailable).unwrap();
        assert_eq!(json, "\"sse_unavailable\"");
        let back: DegradedReason = serde_json::from_str("\"poll_failed\"").unwrap();
        assert_eq!(back, DegradedReason::PollFailed);
    }
}
