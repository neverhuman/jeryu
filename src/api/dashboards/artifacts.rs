//! Owner: TUI Control-Plane API - artifacts dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::artifacts`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArtifactsDashboard {
    pub items: Vec<ArtifactsItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<ArtifactsSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactsItem {
    pub id: String,
    pub label: String,
    pub artifact_id: String,
    pub kind: String,
    pub verified: bool,
    pub sbom_present: bool,
    pub signature_status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArtifactsSummary {
    pub total_artifacts: u32,
    pub verified_count: u32,
    pub signed_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = ArtifactsDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = ArtifactsDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: ArtifactsDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
