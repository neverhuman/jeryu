//! Owner: TUI Control-Plane API - queue dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::queue`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QueueDashboard {
    pub items: Vec<QueueItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<QueueSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueItem {
    pub id: String,
    pub label: String,
    pub job_id: String,
    pub pipeline_id: String,
    pub age_seconds: u64,
    pub blocked_by: Vec<String>,
    pub queue_depth_above: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QueueSummary {
    pub running_jobs: u32,
    pub queued_jobs: u32,
    pub blocked_jobs: u32,
    pub capacity: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = QueueDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = QueueDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: QueueDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
