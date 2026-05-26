//! Delivery PR and fleet view model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::snapshot::{WorkflowSnapshot, WorkflowSummary};

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

impl PartialOrd for CanonicalPhase {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CanonicalPhase {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let lhs = CanonicalPhase::ALL
            .iter()
            .position(|phase| phase == self)
            .expect("CanonicalPhase::ALL must list every variant");
        let rhs = CanonicalPhase::ALL
            .iter()
            .position(|phase| phase == other)
            .expect("CanonicalPhase::ALL must list every variant");
        lhs.cmp(&rhs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrStatus {
    Draft,
    #[default]
    Open,
    Running,
    Merged,
    Blocked,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestView {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_sha: String,
    pub status: PrStatus,
    pub phase: CanonicalPhase,
    pub mergeable: bool,
    pub ci_summary: WorkflowSummary,
    pub age_secs: u64,
    pub draft: bool,
    pub labels: Vec<String>,
    pub current_node_id: Option<String>,
    pub snapshot: WorkflowSnapshot,
    #[serde(default)]
    pub repo_alias: Option<String>,
    #[serde(default)]
    pub repo_slug: Option<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FleetSummary {
    pub open_prs: u32,
    pub ready_to_ship: u32,
    pub running: u32,
    pub blocked: u32,
    pub merged_today: u32,
    pub canary_in_flight: bool,
    pub prod_in_flight: bool,
    pub canary_url: Option<String>,
    pub top_blocker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverySnapshot {
    pub generated_at: DateTime<Utc>,
    pub pull_requests: Vec<PullRequestView>,
    pub selected_pr_idx: usize,
    pub fleet_summary: FleetSummary,
    pub outdated: bool,
    #[serde(default = "default_kill_bell_state")]
    pub kill_bell_state: String,
}

fn default_kill_bell_state() -> String {
    "armed".to_string()
}

impl DeliverySnapshot {
    pub fn empty() -> Self {
        Self {
            generated_at: Utc::now(),
            pull_requests: Vec::new(),
            selected_pr_idx: 0,
            fleet_summary: FleetSummary::default(),
            outdated: false,
            kill_bell_state: default_kill_bell_state(),
        }
    }

    pub fn selected(&self) -> Option<&PullRequestView> {
        self.pull_requests.get(self.selected_pr_idx)
    }

    pub fn selected_mut(&mut self) -> Option<&mut PullRequestView> {
        self.pull_requests.get_mut(self.selected_pr_idx)
    }

    pub fn next_pr(&mut self) {
        if !self.pull_requests.is_empty() {
            self.selected_pr_idx = (self.selected_pr_idx + 1) % self.pull_requests.len();
        }
    }

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

    pub fn select_by_number(&mut self, number: u64) -> bool {
        if let Some(idx) = self.pull_requests.iter().position(|pr| pr.number == number) {
            self.selected_pr_idx = idx;
            true
        } else {
            false
        }
    }

    pub fn next_pr_matching<F>(&mut self, keep: F)
    where
        F: Fn(&PullRequestView) -> bool,
    {
        if self.pull_requests.is_empty() {
            return;
        }
        let len = self.pull_requests.len();
        for offset in 1..=len {
            let idx = (self.selected_pr_idx + offset) % len;
            if keep(&self.pull_requests[idx]) {
                self.selected_pr_idx = idx;
                return;
            }
        }
    }

    pub fn prev_pr_matching<F>(&mut self, keep: F)
    where
        F: Fn(&PullRequestView) -> bool,
    {
        if self.pull_requests.is_empty() {
            return;
        }
        let len = self.pull_requests.len();
        for offset in 1..=len {
            let idx = (self.selected_pr_idx + len - offset) % len;
            if keep(&self.pull_requests[idx]) {
                self.selected_pr_idx = idx;
                return;
            }
        }
    }

    pub fn ensure_selection_matches<F>(&mut self, keep: F)
    where
        F: Fn(&PullRequestView) -> bool,
    {
        if self.pull_requests.is_empty() {
            return;
        }
        if let Some(pr) = self.pull_requests.get(self.selected_pr_idx)
            && keep(pr)
        {
            return;
        }
        if let Some(idx) = self.pull_requests.iter().position(&keep) {
            self.selected_pr_idx = idx;
        }
    }

    pub fn count_matching<F>(&self, keep: F) -> usize
    where
        F: Fn(&PullRequestView) -> bool,
    {
        self.pull_requests.iter().filter(|pr| keep(pr)).count()
    }
}
