//! Owner: TUI Control-Plane API - release dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::release`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::entity::HealthLevel;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReleaseDashboard {
    pub items: Vec<ReleaseItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<ReleaseSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseItem {
    pub id: String,
    pub label: String,
    pub release_id: String,
    pub candidate_sha: String,
    pub status: String,
    pub gate: String,
    pub rollback_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseSummary {
    pub candidate_ready: bool,
    pub canary_passing: bool,
    pub production_health: HealthLevel,
}

impl Default for ReleaseSummary {
    fn default() -> Self {
        Self {
            candidate_ready: false,
            canary_passing: false,
            production_health: HealthLevel::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = ReleaseDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = ReleaseDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: ReleaseDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
