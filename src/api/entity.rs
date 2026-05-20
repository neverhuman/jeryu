//! Owner: TUI Control-Plane API — unified entity model
//! Proof: `cargo nextest run -p jeryu -- api::entity`
//! Invariants: Every TUI-rendered object maps to exactly one `EntityKind`; entity IDs are globally unique within kind.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::tui::action_registry::RiskTier;

// ── Entity Reference ────────────────────────────────────────────────────

/// Lightweight pointer to any entity in the control plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
}

impl EntityRef {
    pub fn new(kind: EntityKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    /// Human-friendly display: `job:14445`, `agent:wrath-17`, etc.
    pub fn display(&self) -> String {
        format!("{}:{}", self.kind.label(), self.id)
    }
}

impl std::fmt::Display for EntityRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind.label(), self.id)
    }
}

// ── Entity Kinds ────────────────────────────────────────────────────────

/// Exhaustive taxonomy of control-plane entities.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Job,
    Pipeline,
    Agent,
    AgentTask,
    MergeRequest,
    TestPlan,
    TestCase,
    EvidenceCapsule,
    ReleaseAttempt,
    ReleaseGate,
    CacheTaint,
    CacheObject,
    Bug,
    BugAttempt,
    Project,
    SecretAccess,
    Grant,
    Pool,
    Runner,
    System,
}

impl EntityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Pipeline => "pipeline",
            Self::Agent => "agent",
            Self::AgentTask => "agent_task",
            Self::MergeRequest => "mr",
            Self::TestPlan => "test_plan",
            Self::TestCase => "test_case",
            Self::EvidenceCapsule => "capsule",
            Self::ReleaseAttempt => "release",
            Self::ReleaseGate => "gate",
            Self::CacheTaint => "taint",
            Self::CacheObject => "cache_object",
            Self::Bug => "bug",
            Self::BugAttempt => "bug_attempt",
            Self::Project => "project",
            Self::SecretAccess => "secret",
            Self::Grant => "grant",
            Self::Pool => "pool",
            Self::Runner => "runner",
            Self::System => "system",
        }
    }
}

// ── Severity ────────────────────────────────────────────────────────────

/// Event/attention severity, ordered from most to least urgent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Blocks release or production; requires immediate action.
    Critical,
    /// Blocks merge or agent progress; should be addressed soon.
    Error,
    /// Degraded state; may self-resolve.
    Warning,
    /// Informational; no action needed.
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "P0",
            Self::Error => "P1",
            Self::Warning => "P2",
            Self::Info => "info",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::Critical => "🚨",
            Self::Error => "✗",
            Self::Warning => "⚠",
            Self::Info => "ℹ",
        }
    }
}

// ── Health Level ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Healthy,
    Warning,
    Degraded,
    Critical,
    Unknown,
}

impl HealthLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "HEALTHY",
            Self::Warning => "WARNING",
            Self::Degraded => "DEGRADED",
            Self::Critical => "CRITICAL",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::Healthy => "◉",
            Self::Warning => "◎",
            Self::Degraded => "◎",
            Self::Critical => "◉",
            Self::Unknown => "◇",
        }
    }
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
    pub stale_after_ms: Option<u64>,
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
            stale_after_ms: None,
        }
    }
}

// ── Supporting types ────────────────────────────────────────────────────

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugAttempt {
    pub id: i64,
    pub bug_id: String,
    pub status: String,
    pub agent: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub alias: String,
    pub repo_slug: String,
    pub provider_kind: String,
    pub default_branch: String,
}

// ── Data Freshness ──────────────────────────────────────────────────────

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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_ref_display() {
        let r = EntityRef::new(EntityKind::Job, "14445");
        assert_eq!(r.display(), "job:14445");
        assert_eq!(format!("{r}"), "job:14445");
    }

    #[test]
    fn entity_kinds_have_unique_labels() {
        use std::collections::HashSet;
        let kinds = [
            EntityKind::Job,
            EntityKind::Pipeline,
            EntityKind::Agent,
            EntityKind::AgentTask,
            EntityKind::MergeRequest,
            EntityKind::TestPlan,
            EntityKind::TestCase,
            EntityKind::EvidenceCapsule,
            EntityKind::ReleaseAttempt,
            EntityKind::ReleaseGate,
            EntityKind::CacheTaint,
            EntityKind::CacheObject,
            EntityKind::Bug,
            EntityKind::BugAttempt,
            EntityKind::Project,
            EntityKind::SecretAccess,
            EntityKind::Grant,
            EntityKind::Pool,
            EntityKind::Runner,
            EntityKind::System,
        ];
        let mut labels = HashSet::new();
        for kind in &kinds {
            assert!(
                labels.insert(kind.label()),
                "duplicate label: {}",
                kind.label()
            );
        }
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Critical < Severity::Error);
        assert!(Severity::Error < Severity::Warning);
        assert!(Severity::Warning < Severity::Info);
    }

    #[test]
    fn entity_detail_default_is_unknown() {
        let detail = EntityDetail::default();
        assert_eq!(detail.state, "unknown");
        assert!(detail.timeline.is_empty());
        assert!(detail.blockers.is_empty());
    }
}
