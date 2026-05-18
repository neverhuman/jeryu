//! Owner: Interactive TUI subsystem — delivery-layer model types
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::model`
//! Invariants: Delivery types are read-only; built by builder, consumed by widget.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::model::{WorkflowSnapshot, WorkflowSummary};

/// Canonical phase names for the end-to-end Delivery view.
///
/// These map a PR's progress through the developer pipeline so the TUI can
/// render a consistent phase rail / minimap independent of how the underlying
/// CI happens to group jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalPhase {
    PreMergeCI,
    AgentReviewPreMerge,
    AutoMerge,
    PostMergeCI,
    AgentReviewPostMerge,
    BuildArtifact,
    PromoteLocal,
    PromoteDev,
    PromoteProd,
    MonitorRollback,
}

impl CanonicalPhase {
    pub const ALL: [CanonicalPhase; 10] = [
        Self::PreMergeCI,
        Self::AgentReviewPreMerge,
        Self::AutoMerge,
        Self::PostMergeCI,
        Self::AgentReviewPostMerge,
        Self::BuildArtifact,
        Self::PromoteLocal,
        Self::PromoteDev,
        Self::PromoteProd,
        Self::MonitorRollback,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::PreMergeCI => "Pre-merge CI",
            Self::AgentReviewPreMerge => "Agent review (pre)",
            Self::AutoMerge => "Auto-merge",
            Self::PostMergeCI => "Post-merge CI",
            Self::AgentReviewPostMerge => "Agent review (post)",
            Self::BuildArtifact => "Build artifact",
            Self::PromoteLocal => "Promote → local",
            Self::PromoteDev => "Promote → dev",
            Self::PromoteProd => "Promote → prod",
            Self::MonitorRollback => "Monitor / rollback",
        }
    }

    /// Short label used by the left-side phase rail (≤ 7 chars).
    pub fn short(self) -> &'static str {
        match self {
            Self::PreMergeCI => "PreCI",
            Self::AgentReviewPreMerge => "Agent▲",
            Self::AutoMerge => "Merge",
            Self::PostMergeCI => "PostCI",
            Self::AgentReviewPostMerge => "Agent▼",
            Self::BuildArtifact => "Build",
            Self::PromoteLocal => "Local",
            Self::PromoteDev => "Dev",
            Self::PromoteProd => "Prod",
            Self::MonitorRollback => "Watch",
        }
    }

    /// Stable id string for use in phase/node keys.
    pub fn slug(self) -> &'static str {
        match self {
            Self::PreMergeCI => "pre-merge-ci",
            Self::AgentReviewPreMerge => "agent-review-pre",
            Self::AutoMerge => "auto-merge",
            Self::PostMergeCI => "post-merge-ci",
            Self::AgentReviewPostMerge => "agent-review-post",
            Self::BuildArtifact => "build-artifact",
            Self::PromoteLocal => "promote-local",
            Self::PromoteDev => "promote-dev",
            Self::PromoteProd => "promote-prod",
            Self::MonitorRollback => "monitor",
        }
    }
}

/// Lifecycle status of a pull request as it flows through the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrStatus {
    Draft,
    #[default]
    Open,
    /// Pre-merge CI is currently running.
    Running,
    /// Pre-merge CI passed; auto-merge has fired and post-merge is underway.
    Merged,
    /// CI failed somewhere; PR is blocked until resolved.
    Blocked,
    /// PR was closed without merging.
    Closed,
}

impl PrStatus {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Draft => "✎",
            Self::Open => "○",
            Self::Running => "●",
            Self::Merged => "✓",
            Self::Blocked => "✗",
            Self::Closed => "⊘",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Open => "OPEN",
            Self::Running => "CI",
            Self::Merged => "MERGED",
            Self::Blocked => "BLOCKED",
            Self::Closed => "CLOSED",
        }
    }
}

/// A single pull request flowing through the canonical pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestView {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_sha: String,
    pub status: PrStatus,
    /// Furthest canonical phase the PR has reached (passed or currently in).
    pub phase: CanonicalPhase,
    pub mergeable: bool,
    pub ci_summary: WorkflowSummary,
    pub age_secs: u64,
    pub draft: bool,
    pub labels: Vec<String>,
    /// Node within `snapshot` that should be auto-focused when this PR is selected.
    pub current_node_id: Option<String>,
    /// Full canonical-pipeline DAG snapshot for this PR.
    pub snapshot: WorkflowSnapshot,
    /// Risk classification (e.g. "R1", "R2", "R3") surfaced by the autonomy
    /// scorecard. Defaults to "R2" when no classifier output is wired.
    #[serde(default = "default_risk")]
    pub risk: String,
}

fn default_risk() -> String {
    "R2".to_string()
}

impl PullRequestView {
    pub fn short_title(&self, max: usize) -> String {
        if self.title.len() <= max {
            self.title.clone()
        } else {
            let cut = max.saturating_sub(1).min(self.title.len());
            format!("{}…", &self.title[..cut])
        }
    }
}

/// Fleet-wide rollup across every active pull request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FleetSummary {
    pub open_prs: u32,
    pub ready_to_ship: u32,
    pub running: u32,
    pub blocked: u32,
    pub merged_today: u32,
    /// True when a canary deployment is currently in progress.
    pub canary_in_flight: bool,
    /// True when a production deployment is currently in progress.
    pub prod_in_flight: bool,
    /// Most recent canary URL (if any).
    pub canary_url: Option<String>,
    /// Most-blocked node (debug summary, e.g. "build-web · blocks 7").
    pub top_blocker: Option<String>,
}

/// Top-level snapshot consumed by the Delivery view: every active PR + fleet
/// rollup + optional release state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverySnapshot {
    pub generated_at: DateTime<Utc>,
    pub pull_requests: Vec<PullRequestView>,
    /// Index into `pull_requests` for the currently focused PR.
    pub selected_pr_idx: usize,
    pub fleet_summary: FleetSummary,
    /// True when the snapshot is older than its expected refresh interval.
    pub outdated: bool,
    /// Currently active autonomy profile (e.g. "default", "merge-only").
    #[serde(default = "default_profile")]
    pub active_profile: String,
    /// Kill-bell posture, surfaced as a short label like "armed", "paused".
    #[serde(default = "default_kill_bell_state")]
    pub kill_bell_state: String,
}

fn default_profile() -> String {
    "default".to_string()
}

fn default_kill_bell_state() -> String {
    "armed".to_string()
}

impl DeliverySnapshot {
    /// An empty snapshot — no active PRs.
    pub fn empty() -> Self {
        Self {
            generated_at: Utc::now(),
            pull_requests: Vec::new(),
            selected_pr_idx: 0,
            fleet_summary: FleetSummary::default(),
            outdated: false,
            active_profile: default_profile(),
            kill_bell_state: default_kill_bell_state(),
        }
    }

    pub fn selected(&self) -> Option<&PullRequestView> {
        self.pull_requests.get(self.selected_pr_idx)
    }

    pub fn selected_mut(&mut self) -> Option<&mut PullRequestView> {
        self.pull_requests.get_mut(self.selected_pr_idx)
    }

    /// Move selection to the next PR (wraps).
    pub fn next_pr(&mut self) {
        if self.pull_requests.is_empty() {
            return;
        }
        self.selected_pr_idx = (self.selected_pr_idx + 1) % self.pull_requests.len();
    }

    /// Move selection to the previous PR (wraps).
    pub fn prev_pr(&mut self) {
        if self.pull_requests.is_empty() {
            return;
        }
        self.selected_pr_idx = if self.selected_pr_idx == 0 {
            self.pull_requests.len() - 1
        } else {
            self.selected_pr_idx - 1
        };
    }

    /// Select the PR with this number, if present.
    pub fn select_by_number(&mut self, number: u64) -> bool {
        if let Some(idx) = self.pull_requests.iter().position(|pr| pr.number == number) {
            self.selected_pr_idx = idx;
            true
        } else {
            false
        }
    }
}
