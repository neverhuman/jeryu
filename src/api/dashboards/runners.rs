//! Owner: TUI Control-Plane API - runners dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::runners`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::entity::HealthLevel;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RunnersDashboard {
    pub items: Vec<RunnersItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<RunnersSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunnersItem {
    pub id: String,
    pub label: String,
    pub runner_id: String,
    pub pool: String,
    pub status: HealthLevel,
    pub tags: Vec<String>,
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RunnersSummary {
    pub total_runners: u32,
    pub active_runners: u32,
    pub paused_runners: u32,
    pub draining_runners: u32,
    #[serde(default)]
    pub degraded_runners: u32,
    #[serde(default)]
    pub orphaned_containers: u32,
    #[serde(default)]
    pub partial_inventory: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = RunnersDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = RunnersDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: RunnersDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
