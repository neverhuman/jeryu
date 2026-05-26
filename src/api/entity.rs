//! Owner: TUI Control-Plane API — unified entity model
//! Proof: `cargo nextest run -p jeryu -- api::entity`
//! Invariants: Every TUI-rendered object maps to exactly one `EntityKind`; entity IDs are globally unique within kind.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::tui::action_registry::RiskTier;

#[path = "entity_kind.rs"]
mod kind;
#[path = "entity_support.rs"]
mod support;
pub use kind::EntityKind;
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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "entity_tests.rs"]
mod tests;
