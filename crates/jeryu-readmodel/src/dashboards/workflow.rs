//! Workflow dashboard contract — delivery posture per repo.
//!
//! Pure data; freshness carried alongside; default = "empty/unavailable". Each
//! item is one in-flight delivery pipeline: its repo, the pull request driving
//! it, and its posture along the delivery DAG.

use serde::{Deserialize, Serialize};

use crate::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkflowSnapshot {
    pub items: Vec<WorkflowItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<WorkflowSummary>,
}

impl WorkflowSnapshot {
    /// Count of delivery pipelines that are blocked.
    pub fn blocked(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.posture == DeliveryPosture::Blocked)
            .count() as u32
    }
}

/// One in-flight delivery pipeline (per repo / PR).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowItem {
    pub pipeline_id: String,
    pub label: String,
    pub repo_slug: String,
    /// GitHub PR number driving this delivery, if any.
    pub pr_number: Option<u64>,
    /// Delivery posture along the DAG.
    pub posture: DeliveryPosture,
    /// The DAG node currently on the critical path, if any.
    pub critical_path_node: Option<String>,
}

impl WorkflowItem {
    pub fn new(pipeline_id: impl Into<String>, repo_slug: impl Into<String>) -> Self {
        let pipeline_id = pipeline_id.into();
        Self {
            label: pipeline_id.clone(),
            pipeline_id,
            repo_slug: repo_slug.into(),
            pr_number: None,
            posture: DeliveryPosture::Idle,
            critical_path_node: None,
        }
    }
}

impl Default for WorkflowItem {
    fn default() -> Self {
        Self::new("unknown", "unknown/unknown")
    }
}

/// Delivery posture along the PR/CI DAG.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPosture {
    /// CI / review running.
    Running,
    /// Gate failed or PR blocked.
    Blocked,
    /// Merged, post-merge / promotion pending.
    Merging,
    /// Delivered to production.
    Delivered,
    /// No delivery activity.
    Idle,
}

impl DeliveryPosture {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Blocked => "BLOCKED",
            Self::Merging => "merging",
            Self::Delivered => "delivered",
            Self::Idle => "idle",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkflowSummary {
    pub total_pipelines: u32,
    pub running_count: u32,
    pub blocked_count: u32,
    pub longest_running_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = WorkflowSnapshot::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
        assert_eq!(d.blocked(), 0);
    }

    #[test]
    fn delivery_posture_round_trips() {
        let json = serde_json::to_string(&DeliveryPosture::Merging).unwrap();
        assert_eq!(json, "\"merging\"");
        let back: DeliveryPosture = serde_json::from_str(&json).unwrap();
        assert_eq!(back, DeliveryPosture::Merging);
    }

    #[test]
    fn uses_pr_number() {
        let mut item = WorkflowItem::new("pipe-1", "core/web");
        item.pr_number = Some(101);
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("pr_number"));
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = WorkflowSnapshot {
            items: vec![WorkflowItem::new("pipe-1", "core/web")],
            freshness: None,
            summary: Some(WorkflowSummary::default()),
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: WorkflowSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
