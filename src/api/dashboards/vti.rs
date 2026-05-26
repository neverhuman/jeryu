//! Owner: TUI Control-Plane API - vti dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::vti`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VtiDashboard {
    pub items: Vec<VtiItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<VtiSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VtiItem {
    pub id: String,
    pub label: String,
    pub plan_id: String,
    pub total_tests: u32,
    pub skipped_tests: u32,
    pub selector_misses: u32,
    pub confidence: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VtiSummary {
    pub total_plans: u32,
    pub low_confidence_plans: u32,
    pub total_misses_24h: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = VtiDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = VtiDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: VtiDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
