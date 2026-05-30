//! Source freshness contract: degraded source state is explicit and
//! serializable so the TUI/web can render freshness badges per panel.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The data source a panel was hydrated from. Provider-neutral: `Scm` is the
/// source-control plane (no vendor name).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Scm,
    StateStore,
    Sandbox,
    Cache,
    Vault,
    Broker,
    Autonomy,
    Webhook,
    ActionRegistry,
    Capability,
    Jankurai,
    Aer,
    Security,
    ArtifactStore,
    McpHttp,
    McpResource,
    InspectionHttp,
    Fixture,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    Live,
    Fresh,
    Aged,
    Stale,
    LastKnown,
    Inferred,
    Partial,
    SourceDown,
    Unknown,
}

impl FreshnessState {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Live => "LIVE",
            Self::Fresh => "FRESH",
            Self::Aged => "AGED",
            Self::Stale => "STALE",
            Self::LastKnown => "LAST KNOWN",
            Self::Inferred => "INFERRED",
            Self::Partial => "PARTIAL",
            Self::SourceDown => "SOURCE DOWN",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn blocks_risky_action(self) -> bool {
        matches!(
            self,
            Self::Aged
                | Self::Stale
                | Self::LastKnown
                | Self::Inferred
                | Self::Partial
                | Self::SourceDown
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceFreshness {
    pub source: SourceKind,
    pub state: FreshnessState,
    pub observed_at: Option<DateTime<Utc>>,
    pub age_ms: Option<u64>,
    pub cursor: Option<String>,
    pub ttl_ms: Option<u64>,
    pub confidence: f64,
    pub last_error: Option<String>,
    pub degraded_reason: Option<String>,
}

impl SourceFreshness {
    pub fn live(source: SourceKind, observed_at: DateTime<Utc>, cursor: impl Into<String>) -> Self {
        Self {
            source,
            state: FreshnessState::Live,
            observed_at: Some(observed_at),
            age_ms: Some(0),
            cursor: Some(cursor.into()),
            ttl_ms: None,
            confidence: 1.0,
            last_error: None,
            degraded_reason: None,
        }
    }

    pub fn source_down(source: SourceKind, reason: impl Into<String>) -> Self {
        Self {
            source,
            state: FreshnessState::SourceDown,
            observed_at: None,
            age_ms: None,
            cursor: None,
            ttl_ms: None,
            confidence: 0.0,
            last_error: Some(reason.into()),
            degraded_reason: Some("source_down".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_badges_match_plan_language() {
        assert_eq!(FreshnessState::Live.badge(), "LIVE");
        assert_eq!(FreshnessState::SourceDown.badge(), "SOURCE DOWN");
    }

    #[test]
    fn aged_or_down_sources_block_risky_actions() {
        assert!(FreshnessState::Aged.blocks_risky_action());
        assert!(FreshnessState::SourceDown.blocks_risky_action());
        assert!(!FreshnessState::Fresh.blocks_risky_action());
    }

    #[test]
    fn source_down_records_reason() {
        let state = SourceFreshness::source_down(SourceKind::Scm, "timeout");
        assert_eq!(state.confidence, 0.0);
        assert_eq!(state.last_error.as_deref(), Some("timeout"));
    }

    #[test]
    fn source_kind_round_trips() {
        let json = serde_json::to_string(&SourceKind::Scm).unwrap();
        assert_eq!(json, "\"scm\"");
        let back: SourceKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SourceKind::Scm);
    }
}
