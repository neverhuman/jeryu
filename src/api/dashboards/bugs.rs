//! Owner: TUI Control-Plane API - bugs dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::bugs`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::entity::Severity;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BugsDashboard {
    pub items: Vec<BugsItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<BugsSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BugsItem {
    pub id: String,
    pub label: String,
    pub bug_id: String,
    pub title: String,
    pub status: String,
    pub severity: Severity,
    pub ready_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BugsSummary {
    pub total_bugs: u32,
    pub ready_bugs: u32,
    pub blocked_bugs: u32,
    pub in_progress_bugs: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = BugsDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = BugsDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: BugsDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
