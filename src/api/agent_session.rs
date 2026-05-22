//! Owner: TUI Control-Plane API — agent session model
//! Proof: `cargo nextest run -p jeryu -- api::agent_session`
//! Invariants: Agent sessions are first-class entities; pipelines are one attribute, not the whole model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::entity::{ActionRef, BlockerSummary, EntityRef, Severity};
use crate::tui::action_registry::RiskTier;

/// First-class agent session model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub objective: String,
    pub state: AgentState,
    pub branch: Option<String>,
    pub mr_iid: Option<i64>,
    pub head_sha: Option<String>,
    pub trust_tier: TrustTier,
    pub current_intent: Option<String>,
    pub current_step: Option<String>,
    pub budget: AgentBudget,
    pub grants: Vec<ActiveGrant>,
    pub confidence: Option<f64>,
    pub risk: RiskTier,
    pub blockers: Vec<BlockerSummary>,
    pub next_action: Option<ActionRef>,
    pub timeline: Vec<AgentTimelineEvent>,
    pub patch_attempts: Vec<PatchAttempt>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for AgentSession {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            objective: String::new(),
            state: AgentState::Spawning,
            branch: None,
            mr_iid: None,
            head_sha: None,
            trust_tier: TrustTier::Untrusted,
            current_intent: None,
            current_step: None,
            budget: AgentBudget::default(),
            grants: Vec::new(),
            confidence: None,
            risk: RiskTier::Low,
            blockers: Vec::new(),
            next_action: None,
            timeline: Vec::new(),
            patch_attempts: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Spawning,
    Diagnosing,
    Patching,
    Validating,
    Racing,
    Blocked,
    WaitingApproval,
    Completed,
    Failed,
    Paused,
}

impl AgentState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Spawning => "SPAWN",
            Self::Diagnosing => "DIAG",
            Self::Patching => "PATCH",
            Self::Validating => "VALID",
            Self::Racing => "RACE",
            Self::Blocked => "BLOCK",
            Self::WaitingApproval => "AWAIT",
            Self::Completed => "DONE",
            Self::Failed => "FAIL",
            Self::Paused => "PAUSE",
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Spawning => "○",
            Self::Diagnosing => "◎",
            Self::Patching => "◉",
            Self::Validating => "●",
            Self::Racing => "⚡",
            Self::Blocked | Self::Failed => "✗",
            Self::WaitingApproval => "◇",
            Self::Completed => "✓",
            Self::Paused => "⏸",
        }
    }
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
    pub fn is_active(self) -> bool {
        !self.is_terminal() && !matches!(self, Self::Paused)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustTier {
    Untrusted,
    Standard,
    Trusted,
    Elevated,
}

impl TrustTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Untrusted => "untrusted",
            Self::Standard => "standard",
            Self::Trusted => "trusted",
            Self::Elevated => "elevated",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentBudget {
    pub time_used_secs: u64,
    pub time_limit_secs: u64,
    pub ci_minutes_used: u32,
    pub ci_minutes_limit: u32,
    pub max_retries: u32,
    pub retries_used: u32,
    pub allowed_paths: Vec<String>,
}

impl AgentBudget {
    pub fn time_pct(&self) -> f64 {
        if self.time_limit_secs == 0 {
            0.0
        } else {
            (self.time_used_secs as f64 / self.time_limit_secs as f64) * 100.0
        }
    }
    pub fn is_exhausted(&self) -> bool {
        (self.time_limit_secs > 0 && self.time_used_secs >= self.time_limit_secs)
            || (self.ci_minutes_limit > 0 && self.ci_minutes_used >= self.ci_minutes_limit)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveGrant {
    pub grant_id: String,
    pub action_id: String,
    pub scope_description: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub kind: String,
    pub summary: String,
    pub severity: Severity,
    pub entity: Option<EntityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchAttempt {
    pub label: String,
    pub branch: String,
    pub status: PatchStatus,
    pub diff_stat: String,
    pub risk: RiskTier,
    pub score: Option<u32>,
    pub pipeline_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatchStatus {
    Proposed,
    Testing,
    Green,
    Failed,
    Winner,
    Archived,
}

impl PatchStatus {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Proposed => "○",
            Self::Testing => "●",
            Self::Green => "✓",
            Self::Failed => "✗",
            Self::Winner => "★",
            Self::Archived => "⊘",
        }
    }
}

#[cfg(test)]
#[path = "agent_session_tests.rs"]
mod tests;
