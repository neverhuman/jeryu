//! Shared support DTOs and the inspector [`EntityDetail`] contract.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::risk::RiskTier;

use super::kind::EntityKind;
use super::refs::{EntityRef, Severity};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub severity: Severity,
    pub entity: Option<EntityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockerSummary {
    pub kind: String,
    pub severity: Severity,
    pub summary: String,
    pub entity: Option<EntityRef>,
    pub recommended_action: Option<ActionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceRef {
    pub kind: String,
    pub id: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionRef {
    pub action_id: String,
    pub label: String,
    pub risk: Option<RiskTier>,
}

impl ActionRef {
    pub fn new(action_id: impl Into<String>, label: impl Into<String>, risk: RiskTier) -> Self {
        Self {
            action_id: action_id.into(),
            label: label.into(),
            risk: Some(risk),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BugAttempt {
    pub id: i64,
    pub bug_id: String,
    pub status: String,
    pub agent: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub alias: String,
    pub repo_slug: String,
    pub provider_kind: String,
    pub default_branch: String,
}

/// Per-source freshness watermarks so the TUI/web can show freshness indicators
/// per panel. Provider-neutral: `scm_ms` is the source-control watermark.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DataFreshness {
    pub scm_ms: Option<u64>,
    pub state_store_ms: Option<u64>,
    pub sandbox_ms: Option<u64>,
    pub cache_ms: Option<u64>,
    pub vault_ms: Option<u64>,
    pub overall_outdated: bool,
}

// ── Entity Detail (Inspector contract) ──────────────────────────────────

/// Full detail payload for the right-side inspector.
/// Every entity kind must populate this structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDetail {
    pub entity: EntityRef,
    pub state: String,
    pub summary: String,
    pub timeline: Vec<TimelineEvent>,
    pub blockers: Vec<BlockerSummary>,
    pub evidence: Vec<EvidenceRef>,
    pub related: Vec<EntityRef>,
    pub available_actions: Vec<ActionRef>,
    pub risk: Option<RiskTier>,
    pub last_updated: Option<DateTime<Utc>>,
    pub expires_after_ms: Option<u64>,
}

impl Default for EntityDetail {
    fn default() -> Self {
        Self {
            entity: EntityRef::new(EntityKind::System, "unknown"),
            state: "unknown".into(),
            summary: String::new(),
            timeline: Vec::new(),
            blockers: Vec::new(),
            evidence: Vec::new(),
            related: Vec::new(),
            available_actions: Vec::new(),
            risk: None,
            last_updated: None,
            expires_after_ms: None,
        }
    }
}
