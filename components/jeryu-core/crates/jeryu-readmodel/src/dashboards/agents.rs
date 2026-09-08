//! Agents dashboard contract — the agent fleet (active/blocked/grants).
//!
//! Pure data; freshness carried alongside; default = "empty/unavailable". Each
//! item is one live agent session with its lifecycle status and the capability
//! grant it is operating under.

use serde::{Deserialize, Serialize};

use crate::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentsSnapshot {
    pub items: Vec<AgentItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<AgentsSummary>,
}

impl AgentsSnapshot {
    /// Count of agent sessions that are currently blocked.
    pub fn blocked(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.status == AgentStatus::Blocked)
            .count() as u32
    }
}

/// One agent session in the fleet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentItem {
    pub session_id: String,
    pub label: String,
    pub status: AgentStatus,
    /// What the agent is currently working on, if anything.
    pub current_task: Option<String>,
    /// Working branch, if the agent has opened one.
    pub branch: Option<String>,
    /// Number of capability grants this session holds.
    pub grants: u32,
}

impl AgentItem {
    pub fn new(session_id: impl Into<String>, status: AgentStatus) -> Self {
        let session_id = session_id.into();
        Self {
            label: session_id.clone(),
            session_id,
            status,
            current_task: None,
            branch: None,
            grants: 0,
        }
    }
}

impl Default for AgentItem {
    fn default() -> Self {
        Self::new("unknown", AgentStatus::Idle)
    }
}

/// Lifecycle status of an agent session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Actively working a task.
    Active,
    /// Waiting on a human / gate / dependency.
    Blocked,
    /// Live but not working a task.
    Idle,
    /// Session has finished.
    Done,
}

impl AgentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Idle => "idle",
            Self::Done => "done",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentsSummary {
    pub total_sessions: u32,
    pub active_sessions: u32,
    pub blocked_sessions: u32,
    pub active_grants: u32,
    /// Are agents currently authorized to write code?
    pub agents_can_code: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = AgentsSnapshot::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
        assert_eq!(d.blocked(), 0);
    }

    #[test]
    fn agent_status_round_trips() {
        let json = serde_json::to_string(&AgentStatus::Blocked).unwrap();
        assert_eq!(json, "\"blocked\"");
        let back: AgentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AgentStatus::Blocked);
    }

    #[test]
    fn blocked_counts_blocked_sessions() {
        let d = AgentsSnapshot {
            items: vec![
                AgentItem::new("a-1", AgentStatus::Active),
                AgentItem::new("a-2", AgentStatus::Blocked),
                AgentItem::new("a-3", AgentStatus::Blocked),
            ],
            freshness: None,
            summary: None,
        };
        assert_eq!(d.blocked(), 2);
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = AgentsSnapshot::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: AgentsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
