//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.
use super::types::{
    ActivePane, ActiveTab, DeliverySourceStatus, EvidenceViewMode, LiveLogState, LogTarget,
    NodeSummary, PendingApproval, PipelineMetrics, PipelineProgressView, ReleaseStageSnapshot,
    ReleaseSubPane, RunnerFeed, StorageBreakdown, TestViewMode,
};
use crate::{
    docker::DockerCtl,
    gitlab_client::GitlabClient,
    release,
    state::{JobEvent, Pool, TuiSession}, // allowlist: TUI session import
};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::sync::watch;

#[derive(Default)]
pub struct TuiStateSnapshot {
    pub pools: Vec<Pool>,
    pub pool_sync_error: Option<String>,
    pub gitlab_ready: bool,
    pub active_containers: usize,
    pub recent_jobs: Vec<JobEvent>,
    pub pipelines: Vec<PipelineMetrics>,
    pub flow: crate::tui::flow::FlowSnapshot,
    pub fleet: crate::repo_fleet::FleetSnapshot,
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
    pub bugs: Vec<crate::bugtracker::BugRecord>,
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
    // TUI connection status: true if state was fetched within the last 10s
    pub agent_connected: bool,
    /// Status of the delivery (PR) source — populated by the live PR
    /// collector when one is configured. Drives the empty-state card in
    /// `workflow::widget::draw_workflow_empty_state`.
    pub delivery_source_status: DeliverySourceStatus,
    /// Jankurai audit snapshot, refreshed from `agent/repo-score.json` and
    /// `agent/score-history.jsonl` on the background sync tick.
    pub jankurai: crate::tui::jankurai::JankuraiSnapshot,
    /// AER (Audit Error Report) findings — refreshed from
    /// `<repo_root>/aer-findings.json` on the background sync tick.
    pub aer: crate::tui::aer::AerSnapshot,
    /// VRC test-selection plan — refreshed from `<repo_root>/vrc-plan.json`.
    pub vrc: crate::tui::vrc::VrcSnapshot,
    /// Witness build-graph summary — refreshed from `.witness/witness-graph.json`.
    pub witness: crate::tui::witness::WitnessSnapshot,
    /// Proof lanes definition + last-run status, sourced from `proof-lanes.toml`.
    pub proof_lanes: crate::tui::proof_lanes::ProofLanesSnapshot,
    /// Active agent sessions, surfaced on the Agents tab via
    /// `widgets::agent_fleet::render_agent_fleet`. Empty when the store
    /// does not yet expose a sessions list — the tab falls back to the
    /// pipeline-based view derived from `agent_pipelines`.
    pub agent_sessions: Vec<crate::api::agent_session::AgentSession>,
    /// Remote SSH node health summaries — populated by the background sync.
    pub remote_nodes: Vec<NodeSummary>,
}

pub struct App {
    pub store: Option<TuiSession>,
    pub docker: DockerCtl,
    pub gitlab: GitlabClient,
    pub autonomy_dir: PathBuf,
    pub llm_secret_resolver: Option<crate::llm::SecretResolver>,
    pub state: TuiStateSnapshot,

    pub active_tab: ActiveTab,
    pub active_pane: ActivePane,
    pub release_subpane: ReleaseSubPane,
    pub selected_approval_index: usize,
    pub selected_pool_index: usize,
    pub selected_pipeline_index: usize,
    pub selected_job_index: usize,
    pub selected_bug_index: usize,
    pub selected_bug_project_index: usize,
    pub bug_sort_mode: crate::tui::bugs::BugSortMode,
    pub selected_job_id: Option<i64>,
    pub selected_secret_index: usize,
    pub selected_git_index: usize,
    pub selected_repo_index: usize,
    pub repo_detail_open: bool,

    pub maximize_logs: bool,
    pub log_scroll_offset: u16,
    pub follow_log_tail: bool,

    pub test_view_mode: TestViewMode,
    pub selected_test_index: usize,
    pub selected_test_history: Option<Vec<crate::state::TestExecution>>,

    pub selected_evidence_index: usize,
    pub selected_jankurai_index: usize,
    pub selected_palette_index: usize,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub evidence_view_mode: EvidenceViewMode,
    pub focus: crate::tui::focus::FocusState,
    pub focus_map: crate::tui::focus::FocusMap,

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
    /// Feedback line from the most-recent delivery action (rollback, rerun,
    /// etc.). Shown in the Inspector's Actions tab; cleared after a few ticks.
    pub delivery_action_message: Option<String>,

    /// Mission Control action pane (Wave 5 — Evidence Gate). Holds focus,
    /// pending-input buffer, and last-result summary so the cockpit can
    /// surface Approve/Block/Repair/Freeze/KillBell verdicts without a
    /// terminal drop-out.
    pub action_pane: crate::tui::workflow::actions::ActionPaneState,

    /// Side-effect surface for the Mission Control action buttons (Wave
    /// 6.A). Defaults to `FakeActionAdapter` so existing code paths (and
    /// unit tests) keep working without a database; production builds
    /// replace this at startup via [`App::try_install_production_adapter`]
    /// which wires the SQL pool, GitHub client, and signing key behind the
    /// same `ActionAdapter` trait seam.
    pub action_adapter: std::sync::Arc<dyn crate::tui::workflow::action_adapter::ActionAdapter>,

    pub(super) sync_rx: mpsc::Receiver<TuiStateSnapshot>,
    pub(super) sync_tx: mpsc::Sender<TuiStateSnapshot>,

    pub(super) delivery_rx: mpsc::Receiver<crate::tui::workflow::live_delivery::LiveDeliveryUpdate>,
    pub(super) delivery_tx: mpsc::Sender<crate::tui::workflow::live_delivery::LiveDeliveryUpdate>,

    pub(super) log_rx: mpsc::Receiver<LiveLogState>,
    pub(super) log_tx: mpsc::Sender<LiveLogState>,

    pub(super) flow_rx: mpsc::Receiver<crate::tui::flow::FlowSnapshot>,
    pub flow_tx: mpsc::Sender<crate::tui::flow::FlowSnapshot>,

    pub(super) feed_rx: mpsc::Receiver<Vec<RunnerFeed>>,
    pub(super) feed_tx: mpsc::Sender<Vec<RunnerFeed>>,
}
