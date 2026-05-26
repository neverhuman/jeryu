//! Owner: TUI Control-Plane API - workflow dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::workflow`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDashboard {
    pub items: Vec<WorkflowItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<WorkflowSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowItem {
    pub id: String,
    pub label: String,
    pub pipeline_id: String,
    pub repo_slug: String,
    pub mr_iid: Option<String>,
    pub status: String,
    pub critical_path_node: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkflowSummary {
    pub total_pipelines: u32,
    pub blocked_count: u32,
    pub longest_running_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = WorkflowDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = WorkflowDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: WorkflowDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
