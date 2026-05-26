//! Owner: TUI Control-Plane API - cache dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::cache`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CacheDashboard {
    pub items: Vec<CacheItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<CacheSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheItem {
    pub id: String,
    pub label: String,
    pub object_id: String,
    pub category: String,
    pub hit_count: u64,
    pub miss_count: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CacheSummary {
    pub total_objects: u32,
    pub hit_ratio: f64,
    pub total_size_bytes: u64,
    pub taint_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = CacheDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = CacheDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: CacheDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
