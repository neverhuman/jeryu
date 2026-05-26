//! Owner: TUI Control-Plane API - git_sync dashboard contract
//! Proof: `cargo nextest run -p jeryu --lib api::dashboards::git_sync`
//! Invariants: Pure data; freshness carried alongside; default = "empty/unavailable".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GitSyncDashboard {
    pub items: Vec<GitSyncItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<GitSyncSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitSyncItem {
    pub id: String,
    pub label: String,
    pub repo_slug: String,
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    pub last_sync: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GitSyncSummary {
    pub repos_in_sync: u32,
    pub repos_drifted: u32,
    pub mirror_lag_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = GitSyncDashboard::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = GitSyncDashboard::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: GitSyncDashboard = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
