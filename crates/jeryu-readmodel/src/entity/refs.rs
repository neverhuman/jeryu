//! Entity references and the severity / health enums that classify them.

use serde::{Deserialize, Serialize};

use super::kind::EntityKind;

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
}
