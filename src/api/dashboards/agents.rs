//! Owner: TUI Control-Plane API - agents dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::agents`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentsDashboard {
    pub items: Vec<AgentsItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<AgentsSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentsItem {
    pub id: String,
    pub label: String,
    pub session_id: String,
    pub agent_kind: String,
    pub status: String,
    pub current_task: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentsSummary {
    pub total_sessions: u32,
    pub active_sessions: u32,
    pub blocked_sessions: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = AgentsDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = AgentsDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: AgentsDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
