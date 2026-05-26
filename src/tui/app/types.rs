//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.
use crate::state::TrackedPipeline; // allowlist: TUI session import

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ActiveTab {
    #[default]
    Workflow,
    Mission,
    Release,
    Approvals,
    Jobs,
    Agents,
    Tests,
    Pools,
    Cache,
    Evidence,
    Bugs,
    LLMs,
    Git,
    Secrets,
    /// Jankurai audit overview. Reached via Tab / BackTab cycling — no
    /// digit shortcut so the 0-9 layout stays stable.
    Jankurai,
}

impl ActiveTab {
    pub fn from_number(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Workflow),
            1 => Some(Self::Mission),
            2 => Some(Self::Release),
            3 => Some(Self::Approvals),
            4 => Some(Self::Jobs),
            5 => Some(Self::Agents),
            6 => Some(Self::Tests),
            7 => Some(Self::Pools),
            8 => Some(Self::Cache),
            9 => Some(Self::Evidence),
            _ => None,
        }
    }
}

/// Sub-pane within the Release tab. See docs/release-policy.md § TUI surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReleaseSubPane {
    #[default]
    Pipeline,
    Evidence,
    Rollback,
}

impl ReleaseSubPane {
    pub fn next(self) -> Self {
        match self {
            Self::Pipeline => Self::Evidence,
            Self::Evidence => Self::Rollback,
            Self::Rollback => Self::Pipeline,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::Pipeline => Self::Rollback,
            Self::Evidence => Self::Pipeline,
            Self::Rollback => Self::Evidence,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Pipeline => "Pipeline",
            Self::Evidence => "Evidence",
            Self::Rollback => "Rollback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestViewMode {
    #[default]
    Average,
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EvidenceViewMode {
    #[default]
    Capsules,
    AuditLedger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePane {
    Pools,
    Pipelines,
    #[default]
    Jobs,
}

#[derive(Default, Debug, Clone)]
pub struct StorageBreakdown {
    pub docker_images_bytes: u64,
    pub docker_volumes_bytes: u64,
    pub docker_build_cache_bytes: u64,
    pub cas_bytes: u64,
    pub crate_cache_bytes: u64,
    pub runner_data_bytes: u64,
    pub git_repos_bytes: u64,
    pub rust_target_bytes: u64,
    pub state_store_bytes: u64,
    pub total_disk_bytes: u64,
    pub disk_available_bytes: u64,
}

pub struct PipelineMetrics {
    pub pipeline: TrackedPipeline,
    pub total: usize,
    pub completed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogTarget {
    pub project_id: i64,
    pub job_id: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LiveLogState {
    pub target: Option<LogTarget>,
    pub text: String,
    pub updated_at: Option<String>,
    pub error: Option<String>,
    pub outdated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RunnerFeed {
    pub runner_name: String,
    pub job_id: i64,
    pub job_name: String,
    pub pipeline_id: i64,
    pub status: String,
    pub elapsed_secs: f64,
    pub log_tail: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct StageProgress {
    pub stage_name: String,
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub running_jobs: usize,
    pub failed_jobs: usize,
    pub status: String,
    pub avg_duration_secs: Option<f64>,
    pub elapsed_secs: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct PipelineProgressView {
    pub pipeline_id: i64,
    pub ref_name: String,
    pub sha_short: String,
    pub stages: Vec<StageProgress>,
    pub overall_pct: u16,
    pub eta_remaining_secs: Option<u64>,
    pub eta_confidence: String,
    pub wall_clock_secs: u64,
    pub started_at: Option<String>,
}

/// Summary of a remote SSH runner node, shown in the Pools tab node sub-panel.
#[derive(Debug, Clone, Default)]
pub struct NodeSummary {
    pub alias: String,
    pub target: String,
    pub enabled: bool,
    /// `None` = not yet probed this cycle; `Some(true/false)` = last probe result.
    pub reachable: Option<bool>,
    pub active_managers: usize,
    pub max_managers: usize,
    /// Storage used under the node's runner_cache_dir in GiB (None if probe failed).
    pub storage_used_gb: Option<f64>,
    pub storage_limit_gb: f64,
    pub last_contact_at: Option<String>,
}

impl NodeSummary {
    pub fn storage_pct(&self) -> Option<f64> {
        self.storage_used_gb
            .map(|used| (used / self.storage_limit_gb) * 100.0)
    }
}

/// Configuration + last-sync diagnostics for the PR source. Defaults to
/// `configured=false` until the operator wires a GitHub/GitLab source.
#[derive(Debug, Clone, Default)]
pub struct DeliverySourceStatus {
    pub configured: bool,
    pub backend_label: Option<String>,
    pub source_label: Option<String>,
    pub last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_sync_error: Option<String>,
}

/// Counts of in-flight items per stage of the release funnel. Sourced from
/// the state DB + `ops/releases/draft/`. Rendered by the Release → Pipeline
/// sub-pane in the TUI.
#[derive(Debug, Clone, Default)]
pub struct ReleaseStageSnapshot {
    pub plan: Vec<ReleaseStageCard>,
    pub build: Vec<ReleaseStageCard>,
    pub proof: Vec<ReleaseStageCard>,
    pub canary: Vec<ReleaseStageCard>,
    pub stable: Vec<ReleaseStageCard>,
}

impl ReleaseStageSnapshot {
    pub fn total(&self) -> usize {
        self.plan.len()
            + self.build.len()
            + self.proof.len()
            + self.canary.len()
            + self.stable.len()
    }
}

/// One in-flight unit at a stage. Typically a PR; for the Stable column it is
/// the currently-pointed-to version.
#[derive(Debug, Clone)]
pub struct ReleaseStageCard {
    pub label: String,
    pub agent_id: String,
    pub age: String,
}

/// One PR awaiting human approval after CI green. Rendered by the Approvals tab.
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub pr_number: u64,
    pub title: String,
    pub agent_id: String,
    pub risk_tier: u8,
    pub ci_status: String,
    pub age: String,
    pub head_sha: String,
}
