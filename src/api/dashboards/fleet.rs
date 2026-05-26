//! Owner: TUI Control-Plane API - fleet dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::fleet`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::entity::HealthLevel;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FleetDashboard {
    pub items: Vec<FleetItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<FleetSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FleetItem {
    pub id: String,
    pub label: String,
    pub family: String,
    pub repo_slug: String,
    pub health: HealthLevel,
    pub branch: String,
    pub pipeline_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FleetSummary {
    pub total_repos: u32,
    pub healthy_repos: u32,
    pub blocked_repos: u32,
    pub family_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = FleetDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = FleetDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: FleetDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
