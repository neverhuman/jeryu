//! Workflow node status model.

use serde::{Deserialize, Serialize};

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
