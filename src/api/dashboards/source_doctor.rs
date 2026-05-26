//! Owner: TUI Control-Plane API - source_doctor dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::source_doctor`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::freshness::{SourceFreshness, SourceKind};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SourceDoctorDashboard {
    pub items: Vec<SourceDoctorItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<SourceDoctorSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceDoctorItem {
    pub id: String,
    pub label: String,
    pub source_kind: SourceKind,
    pub state: String,
    pub last_error: Option<String>,
    pub last_observed_at: Option<DateTime<Utc>>,
    pub drift_kind: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SourceDoctorSummary {
    pub sources_total: u32,
    pub sources_healthy: u32,
    pub sources_degraded: u32,
    pub schema_drift_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = SourceDoctorDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = SourceDoctorDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: SourceDoctorDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
