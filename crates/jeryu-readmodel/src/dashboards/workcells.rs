//! Workcells dashboard contract - claim state, branch budgets, and repair
//! posture for the shared workcell control plane.
//!
//! Pure data; freshness carried alongside; default = "empty/unavailable". Each
//! item is a single workcell lease or repair cell with its current claim state,
//! branch budget, runner epoch, and heartbeat health.

use serde::{Deserialize, Serialize};

use crate::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkcellsDashboard {
    pub items: Vec<WorkcellItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<WorkcellsSummary>,
}

impl WorkcellsDashboard {
    pub fn blocked(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.claim_state == WorkcellState::Blocked)
            .count() as u32
    }

    pub fn held(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.claim_state == WorkcellState::Held)
            .count() as u32
    }

    pub fn claimed(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.claim_state == WorkcellState::Claimed)
            .count() as u32
    }

    pub fn heartbeat_healthy(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.heartbeat_healthy)
            .count() as u32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkcellItem {
    pub cell_id: String,
    pub label: String,
    pub claim_state: WorkcellState,
    pub agent_id: String,
    pub workspace_root: String,
    pub repo_roots: Vec<String>,
    pub branch_budget: u32,
    pub branches_open: u32,
    pub git_status_summary: String,
    pub ci_snapshot_age_ms: Option<u64>,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub heartbeat_healthy: bool,
    pub startup_rebased: bool,
    pub startup_main_ref: Option<String>,
    pub startup_base_sha: Option<String>,
    pub startup_head_sha: Option<String>,
    pub failed_run_id: Option<String>,
    pub failed_receipt_id: Option<String>,
    pub allowed_paths: Vec<String>,
    pub failure_log_digest: Option<String>,
}

impl WorkcellItem {
    pub fn new(cell_id: impl Into<String>, label: impl Into<String>) -> Self {
        let cell_id = cell_id.into();
        Self {
            label: label.into(),
            cell_id,
            claim_state: WorkcellState::Ready,
            agent_id: String::new(),
            workspace_root: String::new(),
            repo_roots: Vec::new(),
            branch_budget: 1,
            branches_open: 0,
            git_status_summary: String::new(),
            ci_snapshot_age_ms: None,
            runner_id: String::new(),
            runner_epoch: 0,
            heartbeat_healthy: false,
            startup_rebased: false,
            startup_main_ref: None,
            startup_base_sha: None,
            startup_head_sha: None,
            failed_run_id: None,
            failed_receipt_id: None,
            allowed_paths: Vec::new(),
            failure_log_digest: None,
        }
    }
}

impl Default for WorkcellItem {
    fn default() -> Self {
        Self::new("unknown", "unknown")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkcellState {
    Warming,
    Ready,
    Claimed,
    Held,
    Repairing,
    Blocked,
    Released,
}

impl WorkcellState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warming => "warming",
            Self::Ready => "ready",
            Self::Claimed => "claimed",
            Self::Held => "held",
            Self::Repairing => "repairing",
            Self::Blocked => "BLOCKED",
            Self::Released => "released",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkcellsSummary {
    pub total_workcells: u32,
    pub warming_workcells: u32,
    pub ready_workcells: u32,
    pub claimed_workcells: u32,
    pub held_workcells: u32,
    pub repairing_workcells: u32,
    pub blocked_workcells: u32,
    pub heartbeat_healthy: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::freshness::{FreshnessState, SourceKind};
    use chrono::Utc;

    #[test]
    fn dashboard_default_is_empty() {
        let d = WorkcellsDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
        assert_eq!(d.blocked(), 0);
    }

    #[test]
    fn workcell_state_round_trips() {
        let json = serde_json::to_string(&WorkcellState::Repairing).unwrap();
        assert_eq!(json, "\"repairing\"");
        let back: WorkcellState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, WorkcellState::Repairing);
    }

    #[test]
    fn dashboard_counts_claimed_and_blocked_cells() {
        let mut claimed = WorkcellItem::new("wc-1", "agent-a");
        claimed.claim_state = WorkcellState::Claimed;
        claimed.heartbeat_healthy = true;
        claimed.workspace_root = "/workspace/core/web".into();
        let mut blocked = WorkcellItem::new("wc-2", "agent-b");
        blocked.claim_state = WorkcellState::Blocked;
        let mut held = WorkcellItem::new("wc-3", "agent-c");
        held.claim_state = WorkcellState::Held;

        let d = WorkcellsDashboard {
            items: vec![claimed, blocked, held],
            freshness: Some(SourceFreshness {
                source: SourceKind::Autonomy,
                state: FreshnessState::Live,
                observed_at: Some(Utc::now()),
                age_ms: Some(0),
                cursor: Some("cursor-1".into()),
                ttl_ms: Some(5_000),
                confidence: 1.0,
                last_error: None,
                degraded_reason: None,
            }),
            summary: Some(WorkcellsSummary {
                total_workcells: 3,
                warming_workcells: 0,
                ready_workcells: 0,
                claimed_workcells: 1,
                held_workcells: 1,
                repairing_workcells: 0,
                blocked_workcells: 1,
                heartbeat_healthy: 1,
            }),
        };

        assert_eq!(d.claimed(), 1);
        assert_eq!(d.held(), 1);
        assert_eq!(d.blocked(), 1);
        assert_eq!(d.heartbeat_healthy(), 1);
    }
}
