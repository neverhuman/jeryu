//! Owner: TUI Control-Plane API - bottlenecks dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::bottlenecks`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::capacity::BottleneckClass;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BottlenecksDashboard {
    pub items: Vec<BottlenecksItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<BottlenecksSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BottlenecksItem {
    pub id: String,
    pub label: String,
    pub repo_slug: String,
    pub stage: String,
    pub p95_seconds: f64,
    pub contribution_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BottlenecksSummary {
    pub dominant_class: BottleneckClass,
    pub top_bottleneck_repo: Option<String>,
    pub total_blocked_seconds: u64,
}

impl Default for BottlenecksSummary {
    fn default() -> Self {
        Self {
            dominant_class: BottleneckClass::Capacity,
            top_bottleneck_repo: None,
            total_blocked_seconds: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = BottlenecksDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = BottlenecksDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: BottlenecksDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
