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
    /// Populated only when `kind == AgentReview { .. }`. Projection of the
    /// reviewer's signed approval receipt for display in the Inspector's
    /// `Agent` sub-tab.
    #[serde(default)]
    pub agent_call: Option<AgentCallDetail>,
}

/// TUI projection of an agent reviewer's call. Sourced from
/// `crate::autonomy::types::AgentApprovalReceipt` by the sync layer; not
/// mutated by the TUI itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCallDetail {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub agent_id: Option<String>,
    /// `"pass"` / `"block"` / `"concern"` / etc.
    pub decision: Option<String>,
    pub reason: Option<String>,
    pub raw_response_sha: Option<String>,
    pub findings: Vec<AgentFindingBrief>,
    /// Truncated raw decision JSON. Useful for debugging when the reviewer
    /// produces structured output that can't be summarized in `reason`.
    pub decision_json: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentFindingBrief {
    pub severity: String,
    pub class: String,
    pub file: Option<String>,
}

impl From<&crate::autonomy::types::AgentApprovalReceipt> for AgentCallDetail {
    fn from(r: &crate::autonomy::types::AgentApprovalReceipt) -> Self {
        use crate::autonomy::types::{ReviewDecision, Severity};
        let decision = Some(match r.decision {
            ReviewDecision::Pass => "pass".to_string(),
            ReviewDecision::Concern => "concern".to_string(),
            ReviewDecision::Block => "block".to_string(),
            ReviewDecision::Abstain => "abstain".to_string(),
        });
        let findings = r
            .findings
            .iter()
            .map(|f| AgentFindingBrief {
                severity: match f.severity {
                    Severity::Info => "info".into(),
                    Severity::Low => "low".into(),
                    Severity::Medium => "medium".into(),
                    Severity::High => "high".into(),
                    Severity::Critical => "critical".into(),
                },
                class: f.class.clone(),
                file: Some(f.file.clone()),
            })
            .collect();
        AgentCallDetail {
            model: r.model.clone(),
            provider: r.provider.clone(),
            agent_id: Some(r.agent_id.clone()),
            decision,
            reason: r.reason.clone(),
            raw_response_sha: r.raw_response_sha.clone(),
            findings,
            decision_json: None,
        }
    }
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
            repo_alias: None,
            repo_slug: None,
        }
    }

    #[test]
    fn next_pr_matching_skips_non_matching() {
        let mut snap = DeliverySnapshot::empty();
        let mut a = demo_pr(1);
        a.repo_alias = Some("nht".into());
        let mut b = demo_pr(2);
        b.repo_alias = Some("shared".into());
        let mut c = demo_pr(3);
        c.repo_alias = Some("nht".into());
        snap.pull_requests = vec![a, b, c];
        snap.selected_pr_idx = 0;
        snap.next_pr_matching(|pr| pr.repo_alias.as_deref() == Some("nht"));
        assert_eq!(snap.selected_pr_idx, 2);
    }

    #[test]
    fn ensure_selection_matches_jumps_to_first_matching() {
        let mut snap = DeliverySnapshot::empty();
        let mut a = demo_pr(1);
        a.repo_alias = Some("nht".into());
        let mut b = demo_pr(2);
        b.repo_alias = Some("shared".into());
        snap.pull_requests = vec![a, b];
        snap.selected_pr_idx = 0;
        snap.ensure_selection_matches(|pr| pr.repo_alias.as_deref() == Some("shared"));
        assert_eq!(snap.selected_pr_idx, 1);
    }

    #[test]
    fn count_matching_counts_correctly() {
        let mut snap = DeliverySnapshot::empty();
        let mut a = demo_pr(1);
        a.repo_alias = Some("nht".into());
        let mut b = demo_pr(2);
        b.repo_alias = Some("shared".into());
        let mut c = demo_pr(3);
        c.repo_alias = Some("nht".into());
        snap.pull_requests = vec![a, b, c];
        assert_eq!(
            snap.count_matching(|pr| pr.repo_alias.as_deref() == Some("nht")),
            2
        );
        assert_eq!(snap.count_matching(|_| true), 3);
    }
}
