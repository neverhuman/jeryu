//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu -- tui::app`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.
use crate::{
    docker::DockerCtl,
    gitlab_client::GitlabClient,
    release,
    state::{JobEvent, Pool, TrackedPipeline, TuiSession}, // allowlist: TUI session import
};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio::sync::watch;

// ---------------------------------------------------------------------------
// Demo data helpers (extracted)
// ---------------------------------------------------------------------------

#[path = "app_demo.rs"]
mod app_demo;
pub(crate) use app_demo::*;

const LIVE_LOG_MAX_BYTES: usize = 160_000;
const FEED_MAX_LINES: usize = 80;
const FEED_CYCLE_TICKS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    Git,
    Secrets,
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

#[derive(Default)]
pub struct TuiStateSnapshot {
    pub pools: Vec<Pool>,
    pub gitlab_ready: bool,
    pub active_containers: usize,
    pub recent_jobs: Vec<JobEvent>,
    pub pipelines: Vec<PipelineMetrics>,
    pub flow: crate::tui::flow::FlowSnapshot,
    pub live_log: LiveLogState,
    pub hot_cache_usage_bytes: i64,
    pub cache_hits: i64,
    pub cache_objects_count: i64,
    pub proxy_healthy: bool,
    pub registry_healthy: bool,
    pub mirror_enabled: bool,
    pub ca_mounted: bool,
    pub singleflight_requests: i64,
    pub hit_ratio: f64,
    pub miss_count: i64,
    pub total_requests: i64,
    pub active_taint_count: i64,
    pub detonation_breaches: i64,
    pub cold_execution_downgrades: i64,
    pub cas_disk_bytes: i64,
    pub crate_cache_disk_bytes: i64,
    pub storage_breakdown: StorageBreakdown,
    pub pipeline_eta: Option<String>,
    pub pipeline_progress: u16,
    pub release_status: Option<release::ReleaseAttemptView>,
    pub release_status_generated_at: Option<String>,
    pub test_bottlenecks_avg: Vec<crate::state::TestBottleneck>,
    pub test_bottlenecks_latest: Vec<crate::state::TestBottleneck>,
    // State sync:
    pub last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    pub inspector_capsule: Option<crate::capsule::FailureCapsule>,
    pub inspector_job_id: Option<i64>,
    pub recent_evidence: Vec<crate::state::EvidenceRecord>,
    pub secret_audit_events: Vec<crate::state::SecretAuditEvent>,
    pub agent_pipelines: Vec<crate::state::TrackedPipeline>,
    pub recent_audit_events: Vec<crate::state::EventLog>,
    pub recent_git_events: Vec<crate::state::GitCommandEventRecord>,
    // TUI v2 — live runner feeds:
    pub runner_feeds: Vec<RunnerFeed>,
    pub active_feed_index: usize,
    pub feed_cycle_tick: u64,
    pub feed_auto_cycle: bool,
    // TUI v2 — pipeline progress:
    pub pipeline_progress_view: Option<PipelineProgressView>,
    // TUI v2 — event ticker:
    pub event_ticker_offset: usize,
    // Agent-first release process:
    pub release_stages: ReleaseStageSnapshot,
    pub approvals_queue: Vec<PendingApproval>,
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

pub struct App {
    pub store: TuiSession,
    pub docker: DockerCtl,
    pub gitlab: GitlabClient,
    pub state: TuiStateSnapshot,

    pub active_tab: ActiveTab,
    pub active_pane: ActivePane,
    pub release_subpane: ReleaseSubPane,
    pub selected_approval_index: usize,
    pub selected_pool_index: usize,
    pub selected_pipeline_index: usize,
    pub selected_job_index: usize,
    pub selected_job_id: Option<i64>,

    pub maximize_logs: bool,
    pub log_scroll_offset: u16,
    pub follow_log_tail: bool,

    pub test_view_mode: TestViewMode,
    pub selected_test_index: usize,
    pub selected_test_history: Option<Vec<crate::state::TestExecution>>,

    pub selected_evidence_index: usize,
    pub selected_palette_index: usize,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub evidence_view_mode: EvidenceViewMode,

    pub tick_count: u64,

    pub log_target: Option<LogTarget>,
    pub log_target_tx: watch::Sender<Option<LogTarget>>,

    // TUI v2 — runner feed controls:
    pub feed_scroll_offset: u16,
    pub feed_follow_tail: bool,
    pub feed_pinned: Option<usize>,
    // TUI v2 — interactive:
    pub search_active: bool,
    pub search_query: String,
    pub help_overlay_open: bool,

    // Workflow DAG state:
    pub workflow_nav: crate::tui::workflow::nav::WorkflowNav,
    pub workflow_snapshot: crate::tui::workflow::model::WorkflowSnapshot,
    pub workflow_inspect_open: bool,

    // Delivery view (multi-PR canonical pipeline):
    pub delivery_snapshot: crate::tui::workflow::model::DeliverySnapshot,
    pub inspector_tab: crate::tui::workflow::inspector::InspectorTab,
    pub delivery_hit_map: crate::tui::workflow::hit_map::DeliveryHitMap,
    pub drag_origin: Option<(u16, u16)>,

    sync_rx: mpsc::Receiver<TuiStateSnapshot>,
    sync_tx: mpsc::Sender<TuiStateSnapshot>,

    log_rx: mpsc::Receiver<LiveLogState>,
    log_tx: mpsc::Sender<LiveLogState>,

    flow_rx: mpsc::Receiver<crate::tui::flow::FlowSnapshot>,
    pub flow_tx: mpsc::Sender<crate::tui::flow::FlowSnapshot>,

    feed_rx: mpsc::Receiver<Vec<RunnerFeed>>,
    feed_tx: mpsc::Sender<Vec<RunnerFeed>>,
}

#[path = "app_runtime.rs"]
mod app_runtime;
#[cfg(test)]
pub(crate) use app_runtime::test_app;
