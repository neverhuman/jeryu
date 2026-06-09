//! Agent-run dashboard contract - live driver state and controls.
//!
//! Pure data; freshness carried alongside; default = "empty/unavailable".
//! Each item is one high-level `/api/v1/agent-runs` execution with the I/O mode,
//! live TTY posture, and the controls the backend can still honor.

use serde::{Deserialize, Serialize};

use crate::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentRunsDashboard {
    pub items: Vec<AgentRunItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<AgentRunsSummary>,
}

impl AgentRunsDashboard {
    pub fn running(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.status == AgentRunStatus::Running)
            .count() as u32
    }

    pub fn live_tty(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.io_mode == AgentRunIoMode::Pty && item.tty_status.is_some())
            .count() as u32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRunItem {
    pub run_id: String,
    pub label: String,
    pub status: AgentRunStatus,
    pub io_mode: AgentRunIoMode,
    pub source_kind: AgentRunSourceKind,
    pub workcell_id: Option<String>,
    pub runner_epoch: Option<u64>,
    pub tty_status: Option<String>,
    pub last_event: Option<String>,
    pub output_bytes_used: u64,
    pub output_bytes_limit: u64,
    pub supported_controls: Vec<String>,
}

impl AgentRunItem {
    pub fn new(run_id: impl Into<String>, status: AgentRunStatus) -> Self {
        let run_id = run_id.into();
        Self {
            label: run_id.clone(),
            run_id,
            status,
            io_mode: AgentRunIoMode::Pty,
            source_kind: AgentRunSourceKind::Workcell,
            workcell_id: None,
            runner_epoch: None,
            tty_status: None,
            last_event: None,
            output_bytes_used: 0,
            output_bytes_limit: 0,
            supported_controls: Vec::new(),
        }
    }
}

impl Default for AgentRunItem {
    fn default() -> Self {
        Self::new("unknown", AgentRunStatus::Queued)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatus {
    Queued,
    Running,
    Finished,
    Failed,
    Cancelled,
}

impl AgentRunStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunIoMode {
    Pty,
    Pipe,
}

impl AgentRunIoMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pty => "pty",
            Self::Pipe => "pipe",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunSourceKind {
    Workcell,
    Repo,
    Local,
    Scratch,
}

impl AgentRunSourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Workcell => "workcell",
            Self::Repo => "repo",
            Self::Local => "local",
            Self::Scratch => "scratch",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentRunsSummary {
    pub total_runs: u32,
    pub running_runs: u32,
    pub pty_runs: u32,
    pub live_tty_runs: u32,
    pub controllable_runs: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = AgentRunsDashboard::default();
        assert!(d.items.is_empty());
        assert_eq!(d.running(), 0);
        assert_eq!(d.live_tty(), 0);
    }

    #[test]
    fn run_status_round_trips() {
        let json = serde_json::to_string(&AgentRunStatus::Running).unwrap();
        assert_eq!(json, "\"running\"");
        let back: AgentRunStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AgentRunStatus::Running);
    }

    #[test]
    fn live_tty_counts_pty_runs_with_status() {
        let mut pty = AgentRunItem::new("run-1", AgentRunStatus::Running);
        pty.tty_status = Some("live".into());
        let mut pipe = AgentRunItem::new("run-2", AgentRunStatus::Running);
        pipe.io_mode = AgentRunIoMode::Pipe;
        pipe.tty_status = Some("streaming".into());
        let d = AgentRunsDashboard {
            items: vec![pty, pipe],
            freshness: None,
            summary: None,
        };
        assert_eq!(d.running(), 2);
        assert_eq!(d.live_tty(), 1);
    }
}
