//! Owner: Interactive TUI subsystem — workflow DAG model
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::model`
//! Invariants: WorkflowSnapshot is read-only; built by builder, consumed by widget.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::snapshot::{CacheVerdict, VtiStatus};

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

/// Deployment environment for promotion nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    Local,
    Dev,
    Prod,
}

impl Environment {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Dev => "dev",
            Self::Prod => "prod",
        }
    }
}

/// Which side of the merge boundary an agent-review stub sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStage {
    PreMerge,
    PostMerge,
}

impl AgentStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::PreMerge => "pre-merge",
            Self::PostMerge => "post-merge",
        }
    }
}

/// Classification of workflow nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    Check,
    Build,
    Lint,
    UnitTest,
    IntegrationTest,
    SecurityGate,
    ReleaseGate,
    VtiPlan,
    Sentinel,
    /// Stubbed agent code-review step (pre- or post-merge).
    AgentReview {
        stage: AgentStage,
    },
    /// Automatic-merge policy node (passes when pre-merge CI + agent review pass).
    AutoMerge,
    /// Immutable artifact build (container image, binary, etc.).
    BuildArtifact,
    /// Promote an artifact into a target environment.
    Promote {
        env: Environment,
    },
    /// Post-deploy monitoring + rollback gate.
    Monitor,
    #[default]
    Custom,
}

impl WorkflowNodeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Build => "build",
            Self::Lint => "lint",
            Self::UnitTest => "unit",
            Self::IntegrationTest => "integration",
            Self::SecurityGate => "security",
            Self::ReleaseGate => "release-gate",
            Self::VtiPlan => "vti-plan",
            Self::Sentinel => "sentinel",
            Self::AgentReview { stage } => match stage {
                AgentStage::PreMerge => "agent-review (pre)",
                AgentStage::PostMerge => "agent-review (post)",
            },
            Self::AutoMerge => "auto-merge",
            Self::BuildArtifact => "build-artifact",
            Self::Promote { env } => match env {
                Environment::Local => "promote local",
                Environment::Dev => "promote dev",
                Environment::Prod => "promote prod",
            },
            Self::Monitor => "monitor",
            Self::Custom => "custom",
        }
    }

    /// Accent glyph rendered on the node card.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::AgentReview { .. } => "🤖",
            Self::AutoMerge => "⇲",
            Self::BuildArtifact => "📦",
            Self::Promote { .. } => "🚀",
            Self::Monitor => "📈",
            _ => "",
        }
    }

    /// True if this node represents a deployment action that can be rolled back.
    pub fn is_rollback_eligible(self) -> bool {
        matches!(
            self,
            Self::Promote {
                env: Environment::Dev | Environment::Prod
            }
        )
    }
}

/// A single node in the workflow DAG — one test, check, or gate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub label: String,
    pub command: Option<String>,
    pub kind: WorkflowNodeKind,
    pub status: WorkflowStatus,
    pub required: bool,
    pub critical_path: bool,
    pub deps: Vec<String>,
    pub duration_secs: Option<f64>,
    pub eta_secs: Option<u64>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub backend: Option<WorkflowBackendRef>,
    pub reason: Option<String>,
    pub vti_status: Option<VtiStatus>,
    pub cache_verdict: Option<CacheVerdict>,
    pub progress_pct: Option<u16>,
    pub tags: Vec<String>,
}

/// Where a node's live status comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowBackendRef {
    GitlabJob {
        project_id: i64,
        pipeline_id: i64,
        job_id: i64,
    },
    VtiPlanItem {
        plan_id: i64,
        test_id: String,
    },
    LocalProofLane {
        lane: String,
    },
}

/// A dependency edge in the workflow DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    pub kind: WorkflowEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEdgeKind {
    Dependency,
    StageOrder,
    VtiSkip,
}

/// A horizontal row of parallel nodes at the same dependency depth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub id: String,
    pub title: String,
    pub depth: u32,
    pub node_ids: Vec<String>,
}

/// Aggregate counts for the workflow summary banner.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub total: u32,
    pub passed: u32,
    pub running: u32,
    pub waiting: u32,
    pub error: u32,
    pub skipped: u32,
    pub cached: u32,
    pub blocked: u32,
    pub overall_pct: f64,
    pub eta_secs: Option<u64>,
}

impl WorkflowSummary {
    /// Build summary from node statuses.
    pub fn from_nodes(nodes: &[WorkflowNode]) -> Self {
        let mut s = Self {
            total: nodes.len() as u32,
            ..Default::default()
        };
        for n in nodes {
            match n.status {
                WorkflowStatus::Ran => s.passed += 1,
                WorkflowStatus::Running => s.running += 1,
                WorkflowStatus::Waiting => s.waiting += 1,
                WorkflowStatus::Error => s.error += 1,
                WorkflowStatus::Skipped => s.skipped += 1,
                WorkflowStatus::Cached => s.cached += 1,
                WorkflowStatus::Blocked => s.blocked += 1,
                WorkflowStatus::Unknown => {}
            }
        }
        let terminal = s.passed + s.error + s.skipped + s.cached;
        s.overall_pct = if s.total > 0 {
            (terminal as f64 / s.total as f64) * 100.0
        } else {
            0.0
        };
        s
    }
}

/// Where the workflow data came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSource {
    LatestDbPlan,
    CurrentDiff,
    LivePipeline,
    #[default]
    Demo,
}

/// The complete workflow DAG snapshot consumed by the widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSnapshot {
    pub generated_at: DateTime<Utc>,
    pub title: String,
    pub source: WorkflowSource,
    pub mode: String,
    pub confidence: f64,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub phases: Vec<WorkflowPhase>,
    pub summary: WorkflowSummary,
    pub selected_node_id: Option<String>,
    pub outdated: bool,
}

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
}

impl WorkflowSnapshot {
    /// Create an empty snapshot with no active workflow data.
    pub fn empty() -> Self {
        Self {
            generated_at: Utc::now(),
            title: "No active workflow".into(),
            source: WorkflowSource::Demo,
            mode: "none".into(),
            confidence: 0.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            phases: Vec::new(),
            summary: WorkflowSummary::default(),
            selected_node_id: None,
            outdated: false,
        }
    }

    /// Look up a node by ID.
    pub fn node(&self, id: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find nodes in a specific phase.
    pub fn phase_nodes(&self, phase_idx: usize) -> Vec<&WorkflowNode> {
        match self.phases.get(phase_idx) {
            Some(p) => p.node_ids.iter().filter_map(|id| self.node(id)).collect(),
            None => Vec::new(),
        }
    }

    /// Locate the (phase_idx, node_idx) coordinates of a node id.
    /// Used to restore selection after a snapshot rebuild.
    pub fn locate_node(&self, id: &str) -> Option<(usize, usize)> {
        for (pi, phase) in self.phases.iter().enumerate() {
            if let Some(ni) = phase.node_ids.iter().position(|n| n == id) {
                return Some((pi, ni));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_unique() {
        let all = [
            WorkflowStatus::Waiting,
            WorkflowStatus::Running,
            WorkflowStatus::Ran,
            WorkflowStatus::Error,
            WorkflowStatus::Skipped,
            WorkflowStatus::Cached,
            WorkflowStatus::Blocked,
            WorkflowStatus::Unknown,
        ];
        let labels: Vec<_> = all.iter().map(|s| s.label()).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len());
    }

    #[test]
    fn status_terminal_vs_active() {
        assert!(WorkflowStatus::Ran.is_terminal());
        assert!(!WorkflowStatus::Running.is_terminal());
        assert!(WorkflowStatus::Running.is_active());
    }

    #[test]
    fn summary_from_nodes() {
        let nodes = vec![
            WorkflowNode {
                status: WorkflowStatus::Ran,
                ..Default::default()
            },
            WorkflowNode {
                status: WorkflowStatus::Running,
                ..Default::default()
            },
            WorkflowNode {
                status: WorkflowStatus::Waiting,
                ..Default::default()
            },
            WorkflowNode {
                status: WorkflowStatus::Error,
                ..Default::default()
            },
        ];
        let s = WorkflowSummary::from_nodes(&nodes);
        assert_eq!(s.total, 4);
        assert_eq!(s.passed, 1);
        assert!((s.overall_pct - 50.0).abs() < 0.1);
    }

    #[test]
    fn empty_snapshot_is_demo() {
        let snap = WorkflowSnapshot::empty();
        assert_eq!(snap.source, WorkflowSource::Demo);
        assert!(snap.nodes.is_empty());
    }

    #[test]
    fn node_lookup() {
        let mut snap = WorkflowSnapshot::empty();
        snap.nodes.push(WorkflowNode {
            id: "x".into(),
            ..Default::default()
        });
        assert!(snap.node("x").is_some());
        assert!(snap.node("y").is_none());
    }

    #[test]
    fn canonical_phases_have_unique_slugs() {
        let slugs: std::collections::HashSet<_> =
            CanonicalPhase::ALL.iter().map(|p| p.slug()).collect();
        assert_eq!(slugs.len(), CanonicalPhase::ALL.len());
    }

    #[test]
    fn promote_prod_is_rollback_eligible() {
        let prod = WorkflowNodeKind::Promote {
            env: Environment::Prod,
        };
        let dev = WorkflowNodeKind::Promote {
            env: Environment::Dev,
        };
        let local = WorkflowNodeKind::Promote {
            env: Environment::Local,
        };
        let agent = WorkflowNodeKind::AgentReview {
            stage: AgentStage::PreMerge,
        };
        assert!(prod.is_rollback_eligible());
        assert!(dev.is_rollback_eligible());
        assert!(!local.is_rollback_eligible());
        assert!(!agent.is_rollback_eligible());
    }

    #[test]
    fn pr_cycle_wraps() {
        let mut snap = DeliverySnapshot::empty();
        snap.pull_requests = vec![demo_pr(1), demo_pr(2), demo_pr(3)];

        assert_eq!(snap.selected_pr_idx, 0);
        snap.next_pr();
        assert_eq!(snap.selected_pr_idx, 1);
        snap.next_pr();
        snap.next_pr();
        assert_eq!(snap.selected_pr_idx, 0, "next from last wraps to first");

        snap.prev_pr();
        assert_eq!(snap.selected_pr_idx, 2, "prev from first wraps to last");
    }

    #[test]
    fn pr_select_by_number() {
        let mut snap = DeliverySnapshot::empty();
        snap.pull_requests = vec![demo_pr(101), demo_pr(202), demo_pr(303)];
        assert!(snap.select_by_number(202));
        assert_eq!(snap.selected_pr_idx, 1);
        assert!(!snap.select_by_number(999));
    }

    #[test]
    fn pr_next_on_empty_is_noop() {
        let mut snap = DeliverySnapshot::empty();
        snap.next_pr();
        snap.prev_pr();
        assert_eq!(snap.selected_pr_idx, 0);
    }

    fn demo_pr(number: u64) -> PullRequestView {
        PullRequestView {
            number,
            title: format!("PR {}", number),
            author: "alice".into(),
            head_sha: "deadbeef".into(),
            status: PrStatus::Open,
            phase: CanonicalPhase::PreMergeCI,
            mergeable: true,
            ci_summary: WorkflowSummary::default(),
            age_secs: 60,
            draft: false,
            labels: vec![],
            current_node_id: None,
            snapshot: WorkflowSnapshot::empty(),
        }
    }
}
