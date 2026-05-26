//! Owner: TUI Control-Plane API — unified entity model
//! Proof: `cargo nextest run -p jeryu -- api::entity`
//! Invariants: Every TUI-rendered object maps to exactly one `EntityKind`; entity IDs are globally unique within kind.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::tui::action_registry::RiskTier;

#[path = "entity_support.rs"]
mod support;
#[allow(unused_imports)]
pub use support::{
    ActionRef, BlockerSummary, Bug, BugAttempt, DataFreshness, EvidenceRef, Project, TimelineEvent,
};

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
    Repo,
    RepoFamily,
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
            Self::Repo => "repo",
            Self::RepoFamily => "repo_family",
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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "entity_tests.rs"]
mod tests;
