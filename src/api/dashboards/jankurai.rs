//! Owner: TUI Control-Plane API - jankurai dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::jankurai`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::entity::Severity;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JankuraiDashboard {
    pub items: Vec<JankuraiItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<JankuraiSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JankuraiItem {
    pub id: String,
    pub label: String,
    pub finding_id: String,
    pub repo_slug: String,
    pub kind: String,
    pub severity: Severity,
    pub score: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JankuraiSummary {
    pub overall_score: f64,
    pub total_findings: u32,
    pub capped_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = JankuraiDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = JankuraiDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: JankuraiDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
