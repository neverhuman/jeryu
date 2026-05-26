//! Owner: TUI Control-Plane API - autonomy dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::autonomy`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AutonomyDashboard {
    pub items: Vec<AutonomyItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<AutonomySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutonomyItem {
    pub id: String,
    pub label: String,
    pub window_id: String,
    pub kind: String,
    pub state: String,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AutonomySummary {
    pub kill_bell_armed: bool,
    pub freeze_active: bool,
    pub policy_violations: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = AutonomyDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = AutonomyDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: AutonomyDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
