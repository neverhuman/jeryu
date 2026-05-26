//! Owner: TUI Control-Plane API - aer dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::aer`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::entity::Severity;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AerDashboard {
    pub items: Vec<AerItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<AerSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AerItem {
    pub id: String,
    pub label: String,
    pub finding_id: String,
    pub repo_slug: String,
    pub evidence_link: Option<String>,
    pub severity: Severity,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AerSummary {
    pub total_findings: u32,
    pub open_findings: u32,
    pub fixed_findings: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = AerDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = AerDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: AerDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
