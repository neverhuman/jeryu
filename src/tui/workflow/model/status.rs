//! Owner: Interactive TUI subsystem — workflow status enum (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::`

use serde::{Deserialize, Serialize};

/// Canonical status for every workflow node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    #[default]
    Waiting,
    Running,
    Ran,
    Error,
    Skipped,
    Cached,
    Blocked,
    Unknown,
}

impl WorkflowStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => "WAIT",
            Self::Running => "RUN",
            Self::Ran => "RAN",
            Self::Error => "ERR",
            Self::Skipped => "SKIP",
            Self::Cached => "CACHE",
            Self::Blocked => "BLOCK",
            Self::Unknown => "?",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::Waiting => "○",
            Self::Running => "●",
            Self::Ran => "✓",
            Self::Error => "✗",
            Self::Skipped => "⊘",
            Self::Cached => "◈",
            Self::Blocked => "▪",
            Self::Unknown => "◇",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Ran | Self::Error | Self::Skipped | Self::Cached)
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Running)
    }
}
