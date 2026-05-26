//! Owner: TUI Control-Plane API - security dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::security`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::entity::Severity;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SecurityDashboard {
    pub items: Vec<SecurityItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<SecuritySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityItem {
    pub id: String,
    pub label: String,
    pub finding_id: String,
    pub kind: String,
    pub severity: Severity,
    pub status: String,
    pub fixed_in: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SecuritySummary {
    pub open_count: u32,
    pub critical_count: u32,
    pub secrets_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = SecurityDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = SecurityDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: SecurityDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
