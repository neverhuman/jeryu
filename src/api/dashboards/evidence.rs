//! Owner: TUI Control-Plane API - evidence dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::evidence`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::entity::EntityKind;
use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EvidenceDashboard {
    pub items: Vec<EvidenceItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<EvidenceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceItem {
    pub id: String,
    pub label: String,
    pub capsule_id: String,
    pub entity_kind: EntityKind,
    pub entity_id: String,
    pub recorded_at: Option<DateTime<Utc>>,
    pub redacted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EvidenceSummary {
    pub total_capsules: u32,
    pub redacted_count: u32,
    pub last_24h_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = EvidenceDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = EvidenceDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: EvidenceDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
