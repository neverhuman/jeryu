//! Owner: TUI Control-Plane API - llms dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::llms`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LlmsDashboard {
    pub items: Vec<LlmsItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<LlmsSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmsItem {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub spend_dollars: f64,
    pub budget_remaining: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LlmsSummary {
    pub total_calls_24h: u64,
    pub total_spend_dollars: f64,
    pub budget_remaining: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = LlmsDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = LlmsDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: LlmsDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
