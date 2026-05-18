use crate::state::ReleaseAttempt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseAttemptView {
    pub attempt: ReleaseAttempt,
    pub release_dir: String,
    pub canary_state_path: String,
    pub gate_remote_canary_path: String,
    pub gate_canary_e2e_path: String,
    pub gate_canary_telemetry_path: String,
    pub telemetry_diag_path: String,
    pub canary_state: String,
    pub eligibility: String,
    pub phase: Option<String>,
    pub detail: Option<String>,
    pub state_status: Option<String>,
    pub has_remote_gate: bool,
    pub has_telemetry_gate: bool,
    pub has_e2e_gate: bool,
    pub has_telemetry_diag: bool,
    pub release_identity_ok: bool,
    pub canary_public_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseStatusReport {
    pub generated_at: String,
    pub project_id: Option<i64>,
    pub ref_name: Option<String>,
    pub sha: Option<String>,
    pub limit: usize,
    pub total_attempts: usize,
    pub latest: Option<ReleaseAttemptView>,
    pub recent: Vec<ReleaseAttemptView>,
}

#[derive(Debug, Clone)]
pub struct ReleaseStatusQuery {
    pub project_id: Option<i64>,
    pub ref_name: Option<String>,
    pub sha: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CiSchema {
    pub(crate) jobs: Vec<CiSchemaJob>,
    #[serde(default)]
    pub(crate) milestones: Vec<CiSchemaMilestone>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub(crate) struct CiSchemaJob {
    pub(crate) id: String,
    pub(crate) lane: String,
    pub(crate) release_blocking: bool,
    #[serde(default)]
    pub(crate) section: String,
    #[serde(default)]
    pub(crate) summary: String,
    #[serde(default)]
    pub(crate) runner_tags: String,
    #[serde(default)]
    pub(crate) runner_pool: String,
    #[serde(default)]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) component: String,
    #[serde(default)]
    pub(crate) pipeline_product: String,
    #[serde(default)]
    pub(crate) evidence_driven: bool,
    #[serde(default)]
    pub(crate) depends_on: Vec<String>,
    #[serde(default)]
    pub(crate) evidence_outputs: Vec<String>,
    #[serde(default)]
    pub(crate) estimated_cost: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub(crate) struct CiSchemaMilestone {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) lane: String,
    pub(crate) release_blocking: bool,
    #[serde(default)]
    pub(crate) pipeline_product: String,
    pub(crate) jobs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LaneProgress {
    pub passed: usize,
    pub total: usize,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ReleaseExecutionProgress {
    pub percent: f64,
    pub attempt_exists: bool,
    pub remote_gate: bool,
    pub telemetry_gate: bool,
    pub e2e_gate: bool,
    pub punchlist_current: bool,
    pub latest_attempt_sha: Option<String>,
    pub latest_attempt_state: Option<String>,
    pub phase: Option<String>,
    pub eligibility: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressReport {
    pub generated_at: String,
    pub project_id: i64,
    pub ref_name: String,
    pub latest_pipeline_id: Option<i64>,
    pub latest_pipeline_status: Option<String>,
    pub latest_pipeline_sha: Option<String>,
    pub winning_pipeline_id: Option<i64>,
    pub winning_sha: Option<String>,
    pub expected_release_version: Option<String>,
    pub release_critical: LaneProgress,
    pub extended: LaneProgress,
    pub research: LaneProgress,
    pub release_execution: ReleaseExecutionProgress,
    pub blocking_remaining: Vec<String>,
    pub non_blocking_failed: Vec<String>,
    pub current_blocker: Option<String>,
    pub punchlist_freshness: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineExplainItem {
    pub id: String,
    pub status: String,
    pub stage: Option<String>,
    pub runner_pool: String,
    pub kind: String,
    pub component: String,
    pub evidence_driven: bool,
    pub estimated_cost: String,
    pub evidence_outputs: Vec<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineExplainMilestone {
    pub id: String,
    pub title: String,
    pub status: String,
    pub lane: String,
    pub jobs: Vec<String>,
    pub incomplete_jobs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineExplainReport {
    pub generated_at: String,
    pub project_id: i64,
    pub pipeline_id: i64,
    pub pipeline_sha: String,
    pub pipeline_ref: String,
    pub pipeline_status: String,
    pub release_critical: LaneProgress,
    pub extended: LaneProgress,
    pub research: LaneProgress,
    pub release_execution: LaneProgress,
    pub current_blocker: Option<String>,
    pub release_eligible: bool,
    pub blocking_failed: Vec<PipelineExplainItem>,
    pub blocking_pending: Vec<PipelineExplainItem>,
    pub non_blocking_failed: Vec<PipelineExplainItem>,
    pub non_blocking_pending: Vec<PipelineExplainItem>,
    pub incomplete_milestones: Vec<PipelineExplainMilestone>,
    pub untracked_jobs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineDoctorJob {
    pub id: i64,
    pub name: String,
    pub canonical_name: String,
    pub status: String,
    pub stage: String,
    pub runner_pool: String,
    pub runner: Option<String>,
    pub started_at: Option<String>,
    pub duration_secs: Option<f64>,
    pub queued_duration_secs: Option<f64>,
    pub historical_avg_duration_secs: Option<f64>,
    pub historical_max_duration_secs: Option<f64>,
    pub historical_runs: Option<i64>,
    pub slow_factor: Option<f64>,
    pub queue_factor: Option<f64>,
    pub trace_bytes: Option<usize>,
    pub trace_tail: Option<String>,
    pub stuck_suspected: bool,
    pub trace_age_suspected: bool,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineDoctorReport {
    pub generated_at: String,
    pub project_id: i64,
    pub pipeline_id: i64,
    pub pipeline_sha: String,
    pub pipeline_ref: String,
    pub pipeline_status: String,
    pub jobs: Vec<PipelineDoctorJob>,
    pub stuck_suspected: Vec<PipelineDoctorJob>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub(crate) enum ReleaseHealth {
    Blocked,
    Ready,
    Running,
    RemotePassed,
    E2ePassed,
    Failed,
    Outdated,
}
