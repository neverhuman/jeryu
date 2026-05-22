use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub severity: Severity,
    pub entity: Option<EntityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerSummary {
    pub kind: String,
    pub severity: Severity,
    pub summary: String,
    pub entity: Option<EntityRef>,
    pub recommended_action: Option<ActionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub kind: String,
    pub id: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRef {
    pub action_id: String,
    pub label: String,
    pub risk: Option<RiskTier>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bug {
    pub id: String,
    pub title: String,
    pub target_project: String,
    pub source_project: String,
    pub status: String,
    pub severity: String,
    pub priority: String,
    pub difficulty: u8,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugAttempt {
    pub id: i64,
    pub bug_id: String,
    pub status: String,
    pub agent: Option<String>,
    pub branch: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub alias: String,
    pub repo_slug: String,
    pub provider_kind: String,
    pub default_branch: String,
}

/// Per-source freshness watermarks so the TUI can show freshness indicators per panel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataFreshness {
    pub gitlab_ms: Option<u64>,
    pub db_ms: Option<u64>,
    pub docker_ms: Option<u64>,
    pub cache_ms: Option<u64>,
    pub vault_ms: Option<u64>,
    pub overall_stale: bool,
}
