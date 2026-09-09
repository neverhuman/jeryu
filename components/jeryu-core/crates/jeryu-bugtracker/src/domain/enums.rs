//! Bug-domain enumerations: severity, priority, status, sort, and attempt status.
//!
//! Ported verbatim from jeryu's `bugtracker/types_enums.rs`. The status machine,
//! string encodings, and terminal-status set are load-bearing: the store seam and
//! the canonical Markdown renderer depend on them being stable.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BugSeverity {
    S0,
    S1,
    #[default]
    S2,
    S3,
    S4,
}

impl BugSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::S0 => "S0",
            Self::S1 => "S1",
            Self::S2 => "S2",
            Self::S3 => "S3",
            Self::S4 => "S4",
        }
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input.to_ascii_uppercase().as_str() {
            "S0" => Ok(Self::S0),
            "S1" => Ok(Self::S1),
            "S2" => Ok(Self::S2),
            "S3" => Ok(Self::S3),
            "S4" => Ok(Self::S4),
            other => bail!("unknown bug severity '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BugPriority {
    P0,
    P1,
    #[default]
    P2,
    P3,
    P4,
}

impl BugPriority {
    pub fn label(self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
            Self::P4 => "P4",
        }
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input.to_ascii_uppercase().as_str() {
            "P0" => Ok(Self::P0),
            "P1" => Ok(Self::P1),
            "P2" => Ok(Self::P2),
            "P3" => Ok(Self::P3),
            "P4" => Ok(Self::P4),
            other => bail!("unknown bug priority '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BugStatus {
    #[default]
    NeedsTriage,
    NeedsInfo,
    Accepted,
    Ready,
    InProgress,
    Blocked,
    FixProposed,
    Reviewing,
    Verifying,
    Done,
    Duplicate,
    Invalid,
    CannotReproduce,
    WontDo,
}

impl BugStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsTriage => "needs_triage",
            Self::NeedsInfo => "needs_info",
            Self::Accepted => "accepted",
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::FixProposed => "fix_proposed",
            Self::Reviewing => "reviewing",
            Self::Verifying => "verifying",
            Self::Done => "done",
            Self::Duplicate => "duplicate",
            Self::Invalid => "invalid",
            Self::CannotReproduce => "cannot_reproduce",
            Self::WontDo => "wont_do",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Duplicate | Self::Invalid | Self::CannotReproduce | Self::WontDo
        )
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "needs_triage" => Ok(Self::NeedsTriage),
            "needs_info" => Ok(Self::NeedsInfo),
            "accepted" => Ok(Self::Accepted),
            "ready" => Ok(Self::Ready),
            "in_progress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "fix_proposed" => Ok(Self::FixProposed),
            "reviewing" => Ok(Self::Reviewing),
            "verifying" => Ok(Self::Verifying),
            "done" => Ok(Self::Done),
            "duplicate" => Ok(Self::Duplicate),
            "invalid" => Ok(Self::Invalid),
            "cannot_reproduce" => Ok(Self::CannotReproduce),
            "wont_do" => Ok(Self::WontDo),
            other => bail!("unknown bug status '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BugSort {
    #[default]
    Rank,
    Severity,
    Priority,
    Difficulty,
    Ready,
    Updated,
    Attempts,
}

impl BugSort {
    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "rank" => Ok(Self::Rank),
            "severity" => Ok(Self::Severity),
            "priority" => Ok(Self::Priority),
            "difficulty" => Ok(Self::Difficulty),
            "ready" => Ok(Self::Ready),
            "updated" => Ok(Self::Updated),
            "attempts" => Ok(Self::Attempts),
            other => bail!("unknown bug sort '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Pending,
    Started,
    Failed,
    FixProposed,
    Verified,
    Abandoned,
}

impl AttemptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Started => "started",
            Self::Failed => "failed",
            Self::FixProposed => "fix_proposed",
            Self::Verified => "verified",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "pending" => Ok(Self::Pending),
            "started" => Ok(Self::Started),
            "failed" => Ok(Self::Failed),
            "fix_proposed" => Ok(Self::FixProposed),
            "verified" => Ok(Self::Verified),
            "abandoned" => Ok(Self::Abandoned),
            other => bail!("unknown attempt status '{other}'"),
        }
    }
}
