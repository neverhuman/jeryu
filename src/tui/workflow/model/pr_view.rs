//! Owner: Interactive TUI subsystem — workflow PR / delivery view types (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::phase::CanonicalPhase;
use super::snapshot::{WorkflowSnapshot, WorkflowSummary};

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
    /// Fleet alias of the repo that owns this PR (e.g. `"nht"`). `None` when
    /// the source isn't yet repo-aware; such PRs are visible only under
    /// `RepoFilter::All`.
    #[serde(default)]
    pub repo_alias: Option<String>,
    /// Fleet slug of the repo (e.g. `"neverhuman/veox"`). Same semantics as
    /// `repo_alias`.
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
    /// Mission Control mirror of the autonomy Kill Bell state. The TUI
    /// reflects this string (`"armed"`, `"paused"`, …) so operators can
    /// see the current pause posture without polling the autonomy plane.
    /// Default is `"armed"`.
    #[serde(default = "default_kill_bell_state")]
    pub kill_bell_state: String,
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

    /// Move to the next PR that satisfies `keep`, wrapping. No-op when no PR
    /// matches the predicate.
    pub fn next_pr_matching<F>(&mut self, keep: F)
    where
        F: Fn(&PullRequestView) -> bool,
    {
        if self.pull_requests.is_empty() {
            return;
        }
        let n = self.pull_requests.len();
        for offset in 1..=n {
            let i = (self.selected_pr_idx + offset) % n;
            if keep(&self.pull_requests[i]) {
                self.selected_pr_idx = i;
                return;
            }
        }
    }

    /// Move to the previous PR that satisfies `keep`, wrapping.
    pub fn prev_pr_matching<F>(&mut self, keep: F)
    where
        F: Fn(&PullRequestView) -> bool,
    {
        if self.pull_requests.is_empty() {
            return;
        }
        let n = self.pull_requests.len();
        for offset in 1..=n {
            let i = (self.selected_pr_idx + n - offset) % n;
            if keep(&self.pull_requests[i]) {
                self.selected_pr_idx = i;
                return;
            }
        }
    }

    /// If the currently selected PR does not satisfy `keep`, advance to the
    /// first PR that does. No-op if the selection already matches or if no
    /// PR matches at all.
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

    /// How many PRs satisfy `keep`. Used by renderers that report a count
    /// of visible items under the active repo filter.
    pub fn count_matching<F>(&self, keep: F) -> usize
    where
        F: Fn(&PullRequestView) -> bool,
    {
        self.pull_requests.iter().filter(|pr| keep(pr)).count()
    }
}
