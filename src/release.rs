//! Owner: Release Pipeline
//! Proof: `cargo test -p jeryu -- release`
//! Invariants: Exact-SHA evidence matching, canary gate ladder, immutable evidence dirs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::process::Command;
use tracing::{info, warn};

use crate::gitlab_client::{GitlabClient, Job, Pipeline};
use crate::state::{Db, ReleaseAttempt};

/// Typed release pipeline errors for programmatic failure classification.
#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("canary gate rejected for {version}: state is {state} (expected e2e-passed)")]
    CanaryGateRejected { version: String, state: String },

    #[error("missing C artifact handoff for {version} at {path}")]
    MissingHandoff { version: String, path: PathBuf },

    #[error("missing C validation artifact for {version} at {path}")]
    MissingValidation { version: String, path: PathBuf },

    #[error("CI schema command failed: {stderr}")]
    CiSchemaFailed { stderr: String },
}

const DEFAULT_REPO_ROOT: &str = "/home/ubuntu/dougx";
pub const DEFAULT_RELEASE_PROJECT_ID: i64 = 2;

fn repo_root() -> PathBuf {
    std::env::var("JERYU_RELEASE_REPO_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            crate::settings::get()
                .release
                .repo_root
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REPO_ROOT))
}

fn render_release_version(sha: &str) -> String {
    format!("ci-{}", sha.chars().take(12).collect::<String>())
}

fn release_dir(version: &str) -> PathBuf {
    repo_root().join("ops/releases").join(version)
}

fn canary_state_path(version: &str) -> PathBuf {
    release_dir(version).join("deploy-canary-c-state.json")
}

fn gate_remote_canary_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-remote-canary.json")
}

fn gate_canary_e2e_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-e2e.json")
}

fn gate_canary_telemetry_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-telemetry.json")
}

fn gate_prod_promotion_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-prod-promotion.json")
}

fn telemetry_diag_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-telemetry-diagnostics.json")
}

fn c_handoff_path(version: &str) -> PathBuf {
    release_dir(version).join("rendered/c-handoff.json")
}

fn c_validation_path(version: &str) -> PathBuf {
    release_dir(version).join("c-validation.json")
}

/// Download gate files and handoff artifacts from the deploy-canary-final job
/// of a release-execution pipeline to local disk. Non-fatal: logs and returns Ok
/// if the job is not found or individual artifacts are missing.
async fn sync_canary_artifacts(
    client: &GitlabClient,
    project_id: i64,
    release_pipeline_id: i64,
    version: &str,
) -> Result<()> {
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, release_pipeline_id)
        .await?;
    let Some(canary_job) = jobs
        .iter()
        .find(|j| j.name == "deploy-canary-final" && j.status == "success")
    else {
        return Ok(());
    };
    let release_root = release_dir(version);
    if let Err(err) = fs::create_dir_all(&release_root) {
        warn!(version, error = %err, "could not create release dir for artifact sync");
        return Ok(());
    }
    let _ = fs::create_dir_all(release_root.join("rendered"));
    let artifacts = [
        (
            format!("ops/releases/{version}/gate-remote-canary.json"),
            "gate-remote-canary.json",
        ),
        (
            format!("ops/releases/{version}/gate-canary-telemetry.json"),
            "gate-canary-telemetry.json",
        ),
        (
            format!("ops/releases/{version}/gate-canary-e2e.json"),
            "gate-canary-e2e.json",
        ),
        (
            format!("ops/releases/{version}/c-validation.json"),
            "c-validation.json",
        ),
        (
            format!("ops/releases/{version}/deploy-canary-c-state.json"),
            "deploy-canary-c-state.json",
        ),
        (
            format!("ops/releases/{version}/release.json"),
            "release.json",
        ),
        (
            format!("ops/releases/{version}/release.json.sig"),
            "release.json.sig",
        ),
        (
            format!("ops/releases/{version}/release-contract.json"),
            "release-contract.json",
        ),
        (format!("ops/releases/{version}/image.env"), "image.env"),
        (
            format!("ops/releases/{version}/payload-manifest.json"),
            "payload-manifest.json",
        ),
        (format!("ops/releases/{version}/deks.env"), "deks.env"),
        (
            format!("ops/releases/{version}/rendered/c-handoff.json"),
            "rendered/c-handoff.json",
        ),
        (
            format!("ops/releases/{version}/rendered/c-slave.env"),
            "rendered/c-slave.env",
        ),
    ];
    for (artifact_path, local_name) in &artifacts {
        let dest = release_root.join(local_name);
        match client
            .job_artifact_file(project_id, canary_job.id, artifact_path)
            .await
        {
            Ok(content) => {
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Err(err) = fs::write(&dest, content.as_bytes()) {
                    warn!(version, artifact = local_name, error = %err, "could not write synced artifact");
                } else {
                    info!(
                        version,
                        artifact = local_name,
                        "synced canary artifact from CI"
                    );
                }
            }
            Err(err) => {
                warn!(version, artifact = local_name, error = %err, "canary artifact not available in CI");
            }
        }
    }
    Ok(())
}

fn canary_public_url(version: &str) -> Option<String> {
    let raw = fs::read_to_string(c_handoff_path(version)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    for key in [
        "target_url",
        "release_unique_url",
        "unique_canary_url",
        "canary_url",
        "public_url",
    ] {
        if let Some(url) = value.get(key).and_then(|v| v.as_str()) {
            return Some(url.to_string());
        }
    }
    None
}

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
struct CiSchema {
    jobs: Vec<CiSchemaJob>,
    #[serde(default)]
    milestones: Vec<CiSchemaMilestone>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct CiSchemaJob {
    id: String,
    lane: String,
    release_blocking: bool,
    #[serde(default)]
    section: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    runner_tags: String,
    #[serde(default)]
    runner_pool: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    component: String,
    #[serde(default)]
    pipeline_product: String,
    #[serde(default)]
    evidence_driven: bool,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    evidence_outputs: Vec<String>,
    #[serde(default)]
    estimated_cost: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct CiSchemaMilestone {
    id: String,
    title: String,
    lane: String,
    release_blocking: bool,
    #[serde(default)]
    pipeline_product: String,
    jobs: Vec<String>,
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
    pub stale_trace_suspected: bool,
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
enum ReleaseHealth {
    Blocked,
    Ready,
    Running,
    RemotePassed,
    E2ePassed,
    Failed,
    Outdated,
}

impl ReleaseHealth {
    fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::RemotePassed => "remote-passed",
            Self::E2ePassed => "e2e-passed",
            Self::Failed => "failed",
            Self::Outdated => "outdated",
        }
    }

    fn eligibility(self) -> &'static str {
        match self {
            Self::Blocked => "blocked-by-upstream",
            Self::Ready => "ready-for-canary",
            Self::Running => "canary-authorized",
            Self::RemotePassed => "awaiting-final-proof",
            Self::E2ePassed => "released",
            Self::Failed => "requires-investigation",
            Self::Outdated => "artifact-contract-broken",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ReleaseEvidence {
    state_phase: Option<String>,
    state_status: Option<String>,
    state_detail: Option<String>,
    has_remote_gate: bool,
    has_telemetry_gate: bool,
    has_e2e_gate: bool,
    has_c_validation: bool,
    has_c_handoff: bool,
    has_telemetry_diag: bool,
    release_identity_ok: bool,
}

fn release_scope(query: &ReleaseStatusQuery) -> String {
    match (&query.project_id, &query.ref_name, &query.sha) {
        (Some(project_id), Some(ref_name), Some(sha)) => {
            format!("project {project_id} / ref {ref_name} / sha {sha}")
        }
        (Some(project_id), Some(ref_name), None) => {
            format!("project {project_id} / ref {ref_name}")
        }
        (Some(project_id), None, None) => format!("project {project_id}"),
        _ => "all release attempts".to_string(),
    }
}

async fn load_ci_schema(root: &Path) -> Result<CiSchema> {
    let output = Command::new("cargo")
        .current_dir(root)
        .arg("run")
        .arg("-p")
        .arg("veox-testctl")
        .arg("--")
        .arg("ci-schema")
        .output()
        .await
        .context("failed to run veox-testctl ci-schema")?;
    if !output.status.success() {
        return Err(ReleaseError::CiSchemaFailed {
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
        .into());
    }
    serde_json::from_slice(&output.stdout).context("parsing ci-schema JSON")
}

#[derive(Debug, Clone, Default)]
struct AggregatedPipelineJob {
    status: String,
    stage: Option<String>,
}

fn canonical_job_name(name: &str) -> String {
    let Some((prefix, suffix)) = name.rsplit_once(' ') else {
        return name.to_string();
    };
    let Some((lhs, rhs)) = suffix.split_once('/') else {
        return name.to_string();
    };
    if lhs.parse::<usize>().is_ok() && rhs.parse::<usize>().is_ok() {
        prefix.to_string()
    } else {
        name.to_string()
    }
}

fn status_rank(status: &str) -> usize {
    match status {
        "failed" | "canceled" => 6,
        "running" => 5,
        "pending" | "created" | "waiting_for_resource" | "preparing" => 4,
        "manual" => 3,
        "success" => 2,
        "skipped" | "vti-skipped" => 1,
        _ => 0,
    }
}

fn merge_status(current: &str, incoming: &str) -> String {
    if status_rank(incoming) >= status_rank(current) {
        incoming.to_string()
    } else {
        current.to_string()
    }
}

fn aggregate_pipeline_jobs(
    jobs: Vec<crate::gitlab_client::Job>,
) -> HashMap<String, AggregatedPipelineJob> {
    let mut aggregated = HashMap::new();
    for job in jobs {
        let key = canonical_job_name(&job.name);
        aggregated
            .entry(key)
            .and_modify(|current: &mut AggregatedPipelineJob| {
                current.status = merge_status(&current.status, &job.status);
                if current.stage.is_none() {
                    current.stage = Some(job.stage.clone());
                }
            })
            .or_insert_with(|| AggregatedPipelineJob {
                status: job.status,
                stage: Some(job.stage),
            });
    }
    aggregated
}

#[derive(Debug, Deserialize)]
struct VtiSkippedArtifact {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    skipped_jobs: Vec<String>,
    #[serde(default)]
    materialized_jobs: Vec<String>,
}

#[derive(Debug, Default)]
struct VtiGraphMetadata {
    selected_graph: bool,
    materialized_jobs: HashSet<String>,
}

async fn apply_vti_skipped_statuses(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    aggregated: &mut HashMap<String, AggregatedPipelineJob>,
) -> Result<VtiGraphMetadata> {
    let mut metadata = VtiGraphMetadata::default();
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    for job in jobs
        .iter()
        .filter(|job| job.name == "plan-tests" && job.status == "success")
    {
        let Ok(raw) = client
            .job_artifact_file(project_id, job.id, "target/jeryu/vti-skipped.json")
            .await
        else {
            continue;
        };
        let Ok(skipped) = serde_json::from_str::<VtiSkippedArtifact>(&raw) else {
            continue;
        };
        if matches!(skipped.mode.as_deref(), Some("selected" | "docs_only")) {
            metadata.selected_graph = true;
        }
        metadata
            .materialized_jobs
            .extend(skipped.materialized_jobs.into_iter());
        for job_name in skipped.skipped_jobs {
            aggregated
                .entry(job_name)
                .or_insert_with(|| AggregatedPipelineJob {
                    status: "vti-skipped".to_string(),
                    stage: None,
                });
        }
    }
    Ok(metadata)
}

fn apply_vti_selected_omissions(
    schema_jobs: &[CiSchemaJob],
    metadata: &VtiGraphMetadata,
    aggregated: &mut HashMap<String, AggregatedPipelineJob>,
) {
    if !metadata.selected_graph {
        return;
    }
    for job in schema_jobs {
        if metadata.materialized_jobs.contains(&job.id) {
            continue;
        }
        aggregated
            .entry(job.id.clone())
            .or_insert_with(|| AggregatedPipelineJob {
                status: "vti-skipped".to_string(),
                stage: None,
            });
    }
}

/// Fetch, aggregate, and VTI-normalize job statuses for a single pipeline,
/// returning a `HashMap<job_id, status>`. Returns an empty map when `pipeline`
/// is `None`.
async fn collect_pipeline_statuses(
    client: &GitlabClient,
    project_id: i64,
    schema_jobs: &[CiSchemaJob],
    pipeline: Option<&Pipeline>,
) -> Result<HashMap<String, String>> {
    let Some(pipeline) = pipeline else {
        return Ok(HashMap::new());
    };
    let mut aggregated = aggregate_pipeline_jobs(
        client
            .list_pipeline_jobs_with_downstream(project_id, pipeline.id)
            .await?,
    );
    let vti_metadata =
        apply_vti_skipped_statuses(client, project_id, pipeline.id, &mut aggregated).await?;
    apply_vti_selected_omissions(schema_jobs, &vti_metadata, &mut aggregated);
    Ok(aggregated
        .into_iter()
        .map(|(name, state)| (name, state.status))
        .collect())
}

async fn latest_pipeline_for_ref(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<Option<Pipeline>> {
    Ok(client
        .list_pipelines(project_id, Some(ref_name))
        .await?
        .into_iter()
        .next())
}

fn is_release_candidate_job(name: &str) -> bool {
    matches!(
        name,
        "build-enclave-server"
            | "test-live-public-surface"
            | "test-local-built"
            | "publish-rc-dry-run"
            | "test-local-rc"
    )
}

fn jobs_materialize_release_candidate(jobs: &[Job]) -> bool {
    // At least one RC job must have actually succeeded — a skipped or absent RC job
    // means VTI did not select the release surface for this diff.
    jobs.iter()
        .any(|job| is_release_candidate_job(&job.name) && job.status == "success")
}

fn failed_release_candidate_jobs(jobs: &[Job]) -> Vec<String> {
    jobs.iter()
        .filter(|job| is_release_candidate_job(&canonical_job_name(&job.name)))
        .filter(|job| !job.allow_failure)
        .filter(|job| !matches!(job.status.as_str(), "success" | "skipped"))
        .map(|job| job.name.clone())
        .collect()
}

fn aggregated_materializes_release_candidate(
    jobs: &HashMap<String, AggregatedPipelineJob>,
) -> bool {
    jobs.keys().any(|name| is_release_candidate_job(name))
}

async fn latest_release_candidate_pipeline_for_ref(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<Option<Pipeline>> {
    for pipeline in client.list_pipelines(project_id, Some(ref_name)).await? {
        if pipeline.source.as_deref() == Some("parent_pipeline") {
            continue;
        }
        let jobs = client
            .list_pipeline_jobs_with_downstream(project_id, pipeline.id)
            .await?;
        if jobs.iter().any(|job| {
            matches!(
                job.name.as_str(),
                "deploy-canary-final" | "report-testing-punchlist" | "promote-production-final"
            )
        }) {
            continue;
        }
        if pipeline.status != "success" {
            // If all RC jobs were skipped, this is a VTI-only pipeline whose failure
            // (e.g., a non-RC compile job) should not block the release candidate search.
            let rc_jobs: Vec<&Job> = jobs
                .iter()
                .filter(|j| is_release_candidate_job(&j.name))
                .collect();
            let all_rc_skipped =
                !rc_jobs.is_empty() && rc_jobs.iter().all(|j| j.status == "skipped");
            if all_rc_skipped {
                info!(
                    project_id,
                    pipeline_id = pipeline.id,
                    ref_name = %ref_name,
                    status = %pipeline.status,
                    sha = %pipeline.sha,
                    "failed pipeline has all RC jobs skipped (VTI-only); continuing to older pipeline"
                );
                continue;
            }
            info!(
                project_id,
                pipeline_id = pipeline.id,
                ref_name = %ref_name,
                status = %pipeline.status,
                sha = %pipeline.sha,
                "newer ref pipeline is not green; no release candidate is ready"
            );
            return Ok(None);
        }
        let failed_release_jobs = failed_release_candidate_jobs(&jobs);
        if !failed_release_jobs.is_empty() {
            info!(
                project_id,
                pipeline_id = pipeline.id,
                ref_name = %ref_name,
                status = %pipeline.status,
                sha = %pipeline.sha,
                failed_jobs = %failed_release_jobs.join(", "),
                "latest green ref pipeline has failed release-candidate jobs; no release candidate is ready"
            );
            return Ok(None);
        }
        if jobs_materialize_release_candidate(&jobs) {
            return Ok(Some(pipeline));
        }
        // Green pipeline but VTI selected a narrow diff — no RC jobs actually ran.
        // Continue to older pipelines rather than blocking.
        info!(
            project_id,
            pipeline_id = pipeline.id,
            ref_name = %ref_name,
            status = %pipeline.status,
            sha = %pipeline.sha,
            "green pipeline did not materialize RC artifacts (VTI narrow); continuing to older pipeline"
        );
    }
    Ok(None)
}

fn lane_progress(
    schema: &[CiSchemaJob],
    statuses: &HashMap<String, String>,
    lane: &str,
    pipeline_status: &str,
) -> LaneProgress {
    let total = schema
        .iter()
        .filter(|job| job.lane == lane)
        .filter(|job| {
            !matches!(
                effective_progress_status(statuses, &job.id, pipeline_status),
                "omitted" | "skipped" | "vti-skipped"
            )
        })
        .count();
    let passed = schema
        .iter()
        .filter(|job| job.lane == lane)
        .filter(|job| effective_progress_status(statuses, &job.id, pipeline_status) == "success")
        .count();
    let percent = if total == 0 {
        0.0
    } else {
        (passed as f64 / total as f64) * 100.0
    };
    LaneProgress {
        passed,
        total,
        percent,
    }
}

fn effective_progress_status<'a>(
    statuses: &'a HashMap<String, String>,
    job_id: &str,
    pipeline_status: &str,
) -> &'a str {
    match statuses.get(job_id) {
        Some(status) => status.as_str(),
        None => match pipeline_status {
            "success" | "failed" | "canceled" | "skipped" => "omitted",
            _ => "pending",
        },
    }
}

fn read_punchlist_summary(root: &Path) -> Option<serde_json::Value> {
    let path = root.join("testing/status/ci/punchlist_summary_latest.json");
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn punchlist_freshness(root: &Path, winning_sha: Option<&str>, version: Option<&str>) -> String {
    let Some(value) = read_punchlist_summary(root) else {
        return "missing".to_string();
    };
    let summary_sha = value
        .get("winning_sha")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let summary_version = value
        .get("expected_release_version")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let freshness = value
        .get("punchlist_freshness")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    match (winning_sha, version) {
        (Some(winning_sha), Some(version))
            if summary_sha == winning_sha && summary_version == version =>
        {
            freshness.to_string()
        }
        (Some(_), Some(_)) => "outdated-for-sha".to_string(),
        _ => "missing-winning-sha".to_string(),
    }
}

fn summary_lane_progress(summary: &serde_json::Value, key: &str) -> Option<LaneProgress> {
    let section = summary.get(key)?;
    Some(LaneProgress {
        passed: section.get("passed")?.as_u64()? as usize,
        total: section.get("total")?.as_u64()? as usize,
        percent: section.get("percent")?.as_f64()?,
    })
}

fn summary_job_items(
    summary: &serde_json::Value,
    release_blocking: bool,
    failed_only: bool,
) -> Vec<String> {
    let Some(items) = summary.get("milestones").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if item
            .get("release_blocking")
            .and_then(|value| value.as_bool())
            != Some(release_blocking)
        {
            continue;
        }
        let Some(evidence) = item.get("evidence").and_then(|value| value.as_str()) else {
            continue;
        };
        for token in evidence.split(", ") {
            let Some((job, status)) = token.rsplit_once(": ") else {
                continue;
            };
            let include = if failed_only {
                matches!(status, "failed" | "canceled")
            } else {
                status != "passed"
            };
            if include {
                out.push(job.to_string());
            }
        }
    }
    out
}

fn release_execution_percent(progress: &ReleaseExecutionProgress) -> f64 {
    if progress.e2e_gate && progress.punchlist_current {
        100.0
    } else if progress.e2e_gate {
        80.0
    } else if progress.telemetry_gate {
        60.0
    } else if progress.remote_gate {
        40.0
    } else if progress.attempt_exists {
        20.0
    } else {
        0.0
    }
}

fn parse_state_json(version: &str) -> Result<Option<serde_json::Value>> {
    let path = canary_state_path(version);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

fn json_release_identity_ok(path: &Path, version: &str, expected_sha: &str) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    value
        .get("git_sha")
        .and_then(|value| value.as_str())
        .map(|value| value == expected_sha)
        .unwrap_or(false)
        && value
            .get("release_version")
            .and_then(|value| value.as_str())
            .map(|value| value == version)
            .unwrap_or(false)
}

fn release_lock_identity_ok(version: &str, expected_sha: &str) -> bool {
    let Ok(raw) = fs::read_to_string(release_lock_path(version)) else {
        return false;
    };
    let Ok(lock) = serde_json::from_str::<ReleaseLock>(&raw) else {
        return false;
    };
    lock.product_sha == expected_sha && lock.release_version == version
}

fn release_identity_ok(version: &str, expected_sha: &str) -> bool {
    let release_json = release_dir(version).join("release.json");
    let contract_json = release_dir(version).join("release-contract.json");
    release_lock_identity_ok(version, expected_sha)
        && json_release_identity_ok(&release_json, version, expected_sha)
        && json_release_identity_ok(&contract_json, version, expected_sha)
}

#[derive(Debug, Clone, Copy)]
struct CanaryGateFiles {
    remote: bool,
    telemetry: bool,
    e2e: bool,
    validation: bool,
    handoff: bool,
    telemetry_diag: bool,
}

impl CanaryGateFiles {
    fn canary_complete(self) -> bool {
        self.remote && self.telemetry && self.e2e && self.validation
    }

    fn promotion_ready(self) -> bool {
        self.canary_complete() && self.handoff
    }
}

fn canary_gate_files(version: &str) -> CanaryGateFiles {
    CanaryGateFiles {
        remote: gate_remote_canary_path(version).is_file(),
        telemetry: gate_canary_telemetry_path(version).is_file(),
        e2e: gate_canary_e2e_path(version).is_file(),
        validation: c_validation_path(version).is_file(),
        handoff: c_handoff_path(version).is_file(),
        telemetry_diag: telemetry_diag_path(version).is_file(),
    }
}

fn release_evidence(version: &str, expected_sha: &str) -> Result<ReleaseEvidence> {
    let state_value = parse_state_json(version)?;
    let gate_files = canary_gate_files(version);
    Ok(ReleaseEvidence {
        state_phase: state_value
            .as_ref()
            .and_then(|value| value.get("phase"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        state_status: state_value
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        state_detail: state_value
            .as_ref()
            .and_then(|value| value.get("detail"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        has_remote_gate: gate_files.remote,
        has_telemetry_gate: gate_files.telemetry,
        has_e2e_gate: gate_files.e2e,
        has_c_validation: gate_files.validation,
        has_c_handoff: gate_files.handoff,
        has_telemetry_diag: gate_files.telemetry_diag,
        release_identity_ok: release_identity_ok(version, expected_sha),
    })
}

fn has_complete_canary_evidence(evidence: &ReleaseEvidence) -> bool {
    evidence.has_remote_gate
        && evidence.has_telemetry_gate
        && evidence.has_e2e_gate
        && evidence.has_c_validation
        && evidence.has_c_handoff
        && evidence.release_identity_ok
}

fn is_stale_attempt(attempt: &ReleaseAttempt, evidence: &ReleaseEvidence) -> bool {
    if evidence.has_e2e_gate {
        return false;
    }

    let ts = attempt
        .canary_started_at
        .as_deref()
        .or(attempt.canary_finished_at.as_deref())
        .or(Some(attempt.updated_at.as_str()));
    let Some(ts) = ts else {
        return false;
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return false;
    };
    let age = chrono::Utc::now().signed_duration_since(parsed.with_timezone(&chrono::Utc));
    age > chrono::Duration::minutes(30)
}

fn derive_release_health(attempt: &ReleaseAttempt, evidence: &ReleaseEvidence) -> ReleaseHealth {
    if attempt.upstream_status != "success" {
        return ReleaseHealth::Blocked;
    }
    if attempt.canary_status == "passed"
        && attempt.release_pipeline_status.as_deref() == Some("success")
        && has_complete_canary_evidence(evidence)
    {
        return ReleaseHealth::E2ePassed;
    }
    if !evidence.release_identity_ok {
        return ReleaseHealth::Outdated;
    }
    if matches!(evidence.state_status.as_deref(), Some("failed"))
        || attempt.canary_status == "failed"
    {
        return ReleaseHealth::Failed;
    }
    if evidence.has_e2e_gate && has_complete_canary_evidence(evidence) {
        return ReleaseHealth::E2ePassed;
    }
    if evidence.has_remote_gate {
        return ReleaseHealth::RemotePassed;
    }
    if matches!(evidence.state_status.as_deref(), Some("passed")) && !evidence.has_e2e_gate {
        return ReleaseHealth::Outdated;
    }
    if matches!(evidence.state_status.as_deref(), Some("running"))
        || attempt.canary_status == "running"
    {
        return if is_stale_attempt(attempt, evidence) {
            ReleaseHealth::Outdated
        } else {
            ReleaseHealth::Running
        };
    }
    if attempt.canary_status == "pending" {
        return ReleaseHealth::Ready;
    }
    ReleaseHealth::Ready
}

fn derived_note(
    attempt: &ReleaseAttempt,
    evidence: &ReleaseEvidence,
    health: ReleaseHealth,
) -> Option<String> {
    if let Some(detail) = evidence
        .state_detail
        .as_ref()
        .filter(|detail| !detail.trim().is_empty())
    {
        let phase = evidence.state_phase.as_deref().unwrap_or("unknown-phase");
        return Some(format!("{phase}: {detail}"));
    }
    if let Some(note) = attempt
        .canary_note
        .as_ref()
        .filter(|note| !note.trim().is_empty())
    {
        return Some(note.clone());
    }
    if health == ReleaseHealth::Outdated {
        return Some("release evidence is incomplete for this version".to_string());
    }
    None
}

fn view_attempt(attempt: ReleaseAttempt) -> Result<ReleaseAttemptView> {
    let version = attempt.version.clone();
    let evidence = release_evidence(&version, &attempt.sha)?;
    let health = derive_release_health(&attempt, &evidence);
    let detail = derived_note(&attempt, &evidence, health);
    Ok(ReleaseAttemptView {
        attempt,
        release_dir: release_dir(&version).display().to_string(),
        canary_state_path: canary_state_path(&version).display().to_string(),
        gate_remote_canary_path: gate_remote_canary_path(&version).display().to_string(),
        gate_canary_e2e_path: gate_canary_e2e_path(&version).display().to_string(),
        gate_canary_telemetry_path: gate_canary_telemetry_path(&version).display().to_string(),
        telemetry_diag_path: telemetry_diag_path(&version).display().to_string(),
        canary_state: health.as_str().to_string(),
        eligibility: health.eligibility().to_string(),
        phase: evidence.state_phase,
        detail,
        state_status: evidence.state_status,
        has_remote_gate: evidence.has_remote_gate,
        has_telemetry_gate: evidence.has_telemetry_gate,
        has_e2e_gate: evidence.has_e2e_gate,
        has_telemetry_diag: evidence.has_telemetry_diag,
        release_identity_ok: evidence.release_identity_ok,
        canary_public_url: canary_public_url(&version),
    })
}

pub async fn build_release_status_report(
    db: &Db,
    query: ReleaseStatusQuery,
) -> Result<ReleaseStatusReport> {
    let recent = if let Some(sha) = &query.sha {
        let mut attempts = Vec::new();
        if let Some(project_id) = query.project_id {
            if let Some(attempt) = db
                .get_release_attempt(project_id, query.ref_name.as_deref().unwrap_or("main"), sha)
                .await?
            {
                attempts.push(attempt);
            }
        } else {
            attempts = db
                .recent_release_attempts(None, query.ref_name.as_deref(), query.limit as i64)
                .await?;
            attempts.retain(|attempt| attempt.sha == *sha);
        }
        attempts
    } else {
        db.recent_release_attempts(
            query.project_id,
            query.ref_name.as_deref(),
            query.limit as i64,
        )
        .await?
    };

    let latest = recent.first().cloned().map(view_attempt).transpose()?;
    let recent = recent
        .into_iter()
        .map(view_attempt)
        .collect::<Result<Vec<_>>>()?;
    Ok(ReleaseStatusReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id: query.project_id,
        ref_name: query.ref_name,
        sha: query.sha,
        limit: query.limit,
        total_attempts: recent.len(),
        latest,
        recent,
    })
}

pub fn summarize_release_attempt(view: &ReleaseAttemptView) -> String {
    let attempt = &view.attempt;
    let upstream = format!("upstream={}", attempt.upstream_status);
    let release_pipeline = attempt
        .release_pipeline_id
        .map(|id| format!("release_pipeline={id}"))
        .unwrap_or_else(|| "release_pipeline=none".to_string());
    let production_pipeline = attempt
        .production_pipeline_id
        .map(|id| format!("production_pipeline={id}"))
        .unwrap_or_else(|| "production_pipeline=none".to_string());
    let canary = format!("canary={}", attempt.canary_status);
    let evidence = view
        .gate_canary_e2e_path
        .rsplit('/')
        .next()
        .unwrap_or(&view.gate_canary_e2e_path);
    format!(
        "{} {} [{}] {} {} {} {} {}",
        attempt.ref_name,
        attempt.version,
        view.canary_state,
        upstream,
        release_pipeline,
        production_pipeline,
        canary,
        evidence
    )
}

pub fn summarize_release_report(report: &ReleaseStatusReport) -> String {
    if let Some(latest) = &report.latest {
        summarize_release_attempt(latest)
    } else {
        "no release attempts found".to_string()
    }
}

pub fn render_release_status_text(report: &ReleaseStatusReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;

    let _ = writeln!(out, "━━━ jeryu release status ━━━");
    let _ = writeln!(
        out,
        "  Scope:      {}",
        release_scope(&ReleaseStatusQuery {
            project_id: report.project_id,
            ref_name: report.ref_name.clone(),
            sha: report.sha.clone(),
            limit: report.limit,
        })
    );
    let _ = writeln!(out, "  Generated:  {}", report.generated_at);
    let _ = writeln!(out, "  Window:     latest {} attempt(s)", report.limit);
    let _ = writeln!(out);

    if let Some(latest) = &report.latest {
        let attempt = &latest.attempt;
        let _ = writeln!(out, "  Latest:");
        let _ = writeln!(out, "    Version:   {}", attempt.version);
        let _ = writeln!(out, "    SHA:       {}", attempt.sha);
        let _ = writeln!(
            out,
            "    Upstream:  {} (pipeline {:?})",
            attempt.upstream_status, attempt.upstream_pipeline_id
        );
        let _ = writeln!(
            out,
            "    Release:   {} (pipeline {:?})",
            attempt
                .release_pipeline_status
                .as_deref()
                .unwrap_or("(not triggered)"),
            attempt.release_pipeline_id
        );
        let _ = writeln!(
            out,
            "    Prod:      {} (pipeline {:?})",
            attempt
                .production_pipeline_status
                .as_deref()
                .unwrap_or("(not triggered)"),
            attempt.production_pipeline_id
        );
        let _ = writeln!(out, "    Canary:    {}", attempt.canary_status);
        let _ = writeln!(out, "    State:     {}", latest.canary_state);
        let _ = writeln!(out, "    Eligible:  {}", latest.eligibility);
        let _ = writeln!(
            out,
            "    Phase:     {}",
            latest.phase.as_deref().unwrap_or("(unknown)")
        );
        let _ = writeln!(
            out,
            "    StateFile: {}",
            latest.state_status.as_deref().unwrap_or("(missing)")
        );
        let _ = writeln!(
            out,
            "    Gates:     remote={} telemetry={} e2e={} telemetry_diag={} identity_ok={}",
            latest.has_remote_gate,
            latest.has_telemetry_gate,
            latest.has_e2e_gate,
            latest.has_telemetry_diag,
            latest.release_identity_ok
        );
        let _ = writeln!(
            out,
            "    URL:       {}",
            latest.canary_public_url.as_deref().unwrap_or("(pending)")
        );
        let _ = writeln!(
            out,
            "    Started:   {}",
            attempt
                .canary_started_at
                .as_deref()
                .unwrap_or("(not started)")
        );
        let _ = writeln!(
            out,
            "    Finished:  {}",
            attempt
                .canary_finished_at
                .as_deref()
                .unwrap_or("(not finished)")
        );
        let _ = writeln!(
            out,
            "    Note:      {}",
            latest.detail.as_deref().unwrap_or("(none)")
        );
        let _ = writeln!(out, "    Evidence:  {}", latest.canary_state_path);
        let _ = writeln!(out, "    Release:   {}", latest.release_dir);
        let _ = writeln!(out);
    } else {
        let _ = writeln!(out, "  Latest:     (no release attempts found)");
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "  Recent attempts:");
    if report.recent.is_empty() {
        let _ = writeln!(out, "    (none)");
    } else {
        for attempt in &report.recent {
            let a = &attempt.attempt;
            let _ = writeln!(
                out,
                "    [{}] project={} ref={} sha={} version={} upstream={} release={} prod={} canary={} phase={}",
                attempt.canary_state,
                a.project_id,
                a.ref_name,
                a.sha,
                a.version,
                a.upstream_status,
                a.release_pipeline_status
                    .as_deref()
                    .unwrap_or("not-triggered"),
                a.production_pipeline_status
                    .as_deref()
                    .unwrap_or("not-triggered"),
                a.canary_status,
                attempt.phase.as_deref().unwrap_or("unknown"),
            );
        }
    }

    out
}

pub async fn render_release_status(db: &Db, query: ReleaseStatusQuery, json: bool) -> Result<()> {
    let report = build_release_status_report(db, query).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_release_status_text(&report));
    }
    Ok(())
}

pub async fn watch_release_status(
    db: &Db,
    query: ReleaseStatusQuery,
    json: bool,
    interval_secs: u64,
) -> Result<()> {
    use std::io::{self, Write};
    use tokio::time::{Duration, sleep};

    let mut stdout = io::stdout();
    loop {
        let report = build_release_status_report(db, query.clone()).await?;
        write!(stdout, "\x1b[2J\x1b[H")?;
        if json {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&report)?)?;
        } else {
            write!(stdout, "{}", render_release_status_text(&report))?;
        }
        stdout.flush()?;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = sleep(Duration::from_secs(interval_secs)) => {}
        }
    }
    Ok(())
}

pub async fn build_progress_report(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<ProgressReport> {
    let root = repo_root();
    let schema = load_ci_schema(&root).await?;
    let latest_pipeline = latest_pipeline_for_ref(client, project_id, ref_name).await?;
    let winning_pipeline =
        latest_release_candidate_pipeline_for_ref(client, project_id, ref_name).await?;

    let latest_statuses =
        collect_pipeline_statuses(client, project_id, &schema.jobs, latest_pipeline.as_ref())
            .await?;
    let winning_statuses =
        collect_pipeline_statuses(client, project_id, &schema.jobs, winning_pipeline.as_ref())
            .await?;

    let winning_sha = winning_pipeline
        .as_ref()
        .map(|pipeline| pipeline.sha.clone());
    let expected_release_version = winning_sha.as_ref().map(|sha| render_release_version(sha));
    let punchlist_summary = read_punchlist_summary(&root);
    let punchlist_freshness = punchlist_freshness(
        &root,
        winning_sha.as_deref(),
        expected_release_version.as_deref(),
    );
    let use_punchlist_summary = punchlist_freshness.starts_with("current");
    let progress_statuses = if winning_statuses.is_empty() {
        &latest_statuses
    } else {
        &winning_statuses
    };
    let progress_pipeline_status = if winning_statuses.is_empty() {
        latest_pipeline
            .as_ref()
            .map(|pipeline| pipeline.status.as_str())
            .unwrap_or("pending")
    } else {
        winning_pipeline
            .as_ref()
            .map(|pipeline| pipeline.status.as_str())
            .unwrap_or("pending")
    };

    let release_critical = if use_punchlist_summary {
        punchlist_summary
            .as_ref()
            .and_then(|summary| summary_lane_progress(summary, "release_critical_jobs"))
            .unwrap_or_else(|| {
                lane_progress(
                    &schema.jobs,
                    progress_statuses,
                    "release-blocking",
                    progress_pipeline_status,
                )
            })
    } else {
        lane_progress(
            &schema.jobs,
            progress_statuses,
            "release-blocking",
            progress_pipeline_status,
        )
    };
    let extended = if use_punchlist_summary {
        punchlist_summary
            .as_ref()
            .and_then(|summary| summary_lane_progress(summary, "extended_verification"))
            .unwrap_or_else(|| {
                lane_progress(
                    &schema.jobs,
                    progress_statuses,
                    "extended",
                    progress_pipeline_status,
                )
            })
    } else {
        lane_progress(
            &schema.jobs,
            progress_statuses,
            "extended",
            progress_pipeline_status,
        )
    };
    let research = if use_punchlist_summary {
        punchlist_summary
            .as_ref()
            .and_then(|summary| summary_lane_progress(summary, "research_support"))
            .unwrap_or_else(|| {
                lane_progress(
                    &schema.jobs,
                    progress_statuses,
                    "research",
                    progress_pipeline_status,
                )
            })
    } else {
        lane_progress(
            &schema.jobs,
            progress_statuses,
            "research",
            progress_pipeline_status,
        )
    };

    let blocking_remaining = if use_punchlist_summary {
        punchlist_summary
            .as_ref()
            .map(|summary| summary_job_items(summary, true, false))
            .unwrap_or_else(|| {
                schema
                    .jobs
                    .iter()
                    .filter(|job| job.lane == "release-blocking")
                    .filter(|job| {
                        !matches!(
                            effective_progress_status(
                                progress_statuses,
                                &job.id,
                                progress_pipeline_status
                            ),
                            "success" | "skipped" | "omitted" | "vti-skipped"
                        )
                    })
                    .map(|job| job.id.clone())
                    .collect::<Vec<_>>()
            })
    } else {
        schema
            .jobs
            .iter()
            .filter(|job| job.lane == "release-blocking")
            .filter(|job| {
                !matches!(
                    effective_progress_status(progress_statuses, &job.id, progress_pipeline_status),
                    "success" | "skipped" | "omitted" | "vti-skipped"
                )
            })
            .map(|job| job.id.clone())
            .collect::<Vec<_>>()
    };
    let non_blocking_failed = if use_punchlist_summary {
        punchlist_summary
            .as_ref()
            .map(|summary| summary_job_items(summary, false, true))
            .unwrap_or_else(|| {
                schema
                    .jobs
                    .iter()
                    .filter(|job| !job.release_blocking)
                    .filter(|job| {
                        matches!(
                            effective_progress_status(
                                progress_statuses,
                                &job.id,
                                progress_pipeline_status
                            ),
                            "failed" | "canceled"
                        )
                    })
                    .map(|job| job.id.clone())
                    .collect::<Vec<_>>()
            })
    } else {
        schema
            .jobs
            .iter()
            .filter(|job| !job.release_blocking)
            .filter(|job| {
                matches!(
                    effective_progress_status(progress_statuses, &job.id, progress_pipeline_status),
                    "failed" | "canceled"
                )
            })
            .map(|job| job.id.clone())
            .collect::<Vec<_>>()
    };
    let attempt_view = if let Some(sha) = winning_sha.as_ref() {
        build_release_status_report(
            db,
            ReleaseStatusQuery {
                project_id: Some(project_id),
                ref_name: Some(ref_name.to_string()),
                sha: Some(sha.clone()),
                limit: 1,
            },
        )
        .await?
        .latest
    } else {
        None
    };

    let mut release_execution = ReleaseExecutionProgress::default();
    if let Some(attempt) = &attempt_view {
        release_execution.attempt_exists = true;
        release_execution.remote_gate = attempt.has_remote_gate;
        release_execution.telemetry_gate = attempt.has_telemetry_gate;
        release_execution.e2e_gate = attempt.has_e2e_gate;
        release_execution.latest_attempt_sha = Some(attempt.attempt.sha.clone());
        release_execution.latest_attempt_state = Some(attempt.canary_state.clone());
        release_execution.phase = attempt.phase.clone();
        release_execution.eligibility = Some(attempt.eligibility.clone());
    }
    if let Some(summary) = &punchlist_summary
        && let Some(release_evidence) = summary.get("release_evidence")
    {
        release_execution.remote_gate = release_execution.remote_gate
            || release_evidence
                .get("remote_gate")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        release_execution.telemetry_gate = release_execution.telemetry_gate
            || release_evidence
                .get("telemetry_gate")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        release_execution.e2e_gate = release_execution.e2e_gate
            || release_evidence
                .get("e2e_gate")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
    }
    release_execution.punchlist_current = punchlist_freshness.starts_with("current");
    release_execution.percent = release_execution_percent(&release_execution);
    let current_blocker = if let Some(job) = blocking_remaining.first() {
        Some(format!("release-critical job pending: {job}"))
    } else if !release_execution.attempt_exists {
        Some("release attempt missing for winning sha".to_string())
    } else if !release_execution.remote_gate {
        Some("canary remote gate missing".to_string())
    } else if !release_execution.telemetry_gate {
        Some("canary telemetry gate missing".to_string())
    } else if !release_execution.e2e_gate {
        Some("canary e2e gate missing".to_string())
    } else if !release_execution.punchlist_current {
        Some("punchlist is outdated for winning sha".to_string())
    } else {
        None
    };

    Ok(ProgressReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id,
        ref_name: ref_name.to_string(),
        latest_pipeline_id: latest_pipeline.as_ref().map(|pipeline| pipeline.id),
        latest_pipeline_status: latest_pipeline
            .as_ref()
            .map(|pipeline| pipeline.status.clone()),
        latest_pipeline_sha: latest_pipeline
            .as_ref()
            .map(|pipeline| pipeline.sha.clone()),
        winning_pipeline_id: winning_pipeline.as_ref().map(|pipeline| pipeline.id),
        winning_sha,
        expected_release_version,
        release_critical,
        extended,
        research,
        release_execution,
        blocking_remaining,
        non_blocking_failed,
        current_blocker,
        punchlist_freshness,
    })
}

pub fn render_progress_text(report: &ProgressReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;

    let _ = writeln!(out, "━━━ jeryu progress ━━━");
    let _ = writeln!(out, "  Generated:         {}", report.generated_at);
    let _ = writeln!(out, "  Ref:               {}", report.ref_name);
    let _ = writeln!(
        out,
        "  Latest pipeline:   {:?} status={} sha={}",
        report.latest_pipeline_id,
        report.latest_pipeline_status.as_deref().unwrap_or("(none)"),
        report.latest_pipeline_sha.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Winning pipeline:  {:?} sha={} version={}",
        report.winning_pipeline_id,
        report.winning_sha.as_deref().unwrap_or("(none)"),
        report
            .expected_release_version
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  Release-Critical:  {}/{} ({:.1}%)",
        report.release_critical.passed,
        report.release_critical.total,
        report.release_critical.percent
    );
    let _ = writeln!(
        out,
        "  Extended:          {}/{} ({:.1}%)",
        report.extended.passed, report.extended.total, report.extended.percent
    );
    let _ = writeln!(
        out,
        "  Research:          {}/{} ({:.1}%)",
        report.research.passed, report.research.total, report.research.percent
    );
    let _ = writeln!(
        out,
        "  Release Execution: {:.1}% freshness={} phase={}",
        report.release_execution.percent,
        report.punchlist_freshness,
        report
            .release_execution
            .phase
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Latest attempt:    sha={} state={}",
        report
            .release_execution
            .latest_attempt_sha
            .as_deref()
            .unwrap_or("(none)"),
        report
            .release_execution
            .latest_attempt_state
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Current blocker:   {}",
        report.current_blocker.as_deref().unwrap_or("(none)")
    );
    if !report.blocking_remaining.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Blocking remaining:");
        for job in &report.blocking_remaining {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    if !report.non_blocking_failed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Non-blocking failed:");
        for job in &report.non_blocking_failed {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    out
}

fn pipeline_lane_progress(
    schema: &[CiSchemaJob],
    statuses: &HashMap<String, AggregatedPipelineJob>,
    lane: &str,
    pipeline_status: &str,
) -> LaneProgress {
    let mut total = 0usize;
    let mut passed = 0usize;
    for job in schema.iter().filter(|job| job.lane == lane) {
        let status = effective_job_status(statuses.get(&job.id), pipeline_status);
        if matches!(status, "omitted" | "skipped" | "vti-skipped") {
            continue;
        }
        total += 1;
        if status == "success" {
            passed += 1;
        }
    }
    let percent = if total == 0 {
        0.0
    } else {
        (passed as f64 / total as f64) * 100.0
    };
    LaneProgress {
        passed,
        total,
        percent,
    }
}

fn effective_job_status<'a>(
    state: Option<&'a AggregatedPipelineJob>,
    pipeline_status: &str,
) -> &'a str {
    match state {
        Some(state) => state.status.as_str(),
        None => match pipeline_status {
            "success" | "failed" | "canceled" | "skipped" => "omitted",
            _ => "pending",
        },
    }
}

fn pipeline_item(
    job: &CiSchemaJob,
    state: Option<&AggregatedPipelineJob>,
    effective_status: &str,
) -> PipelineExplainItem {
    PipelineExplainItem {
        id: job.id.clone(),
        status: display_job_status(effective_status).to_string(),
        stage: state.and_then(|s| s.stage.clone()),
        runner_pool: job.runner_pool.clone(),
        kind: job.kind.clone(),
        component: job.component.clone(),
        evidence_driven: job.evidence_driven,
        estimated_cost: job.estimated_cost.clone(),
        evidence_outputs: job.evidence_outputs.clone(),
        depends_on: job.depends_on.clone(),
    }
}

fn display_job_status(status: &str) -> &str {
    match status {
        "omitted" => "vti-skipped",
        other => other,
    }
}

pub async fn build_pipeline_explain_report(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<PipelineExplainReport> {
    let root = repo_root();
    let schema = load_ci_schema(&root).await?;
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let mut aggregated = aggregate_pipeline_jobs(
        client
            .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
            .await?,
    );
    let vti_metadata =
        apply_vti_skipped_statuses(client, project_id, pipeline_id, &mut aggregated).await?;
    let pipeline_product = if aggregated.contains_key("promote-production-final") {
        "production-promotion"
    } else if aggregated.contains_key("deploy-canary-final")
        || aggregated.contains_key("report-testing-punchlist")
    {
        "release-execution"
    } else {
        "main-candidate"
    };
    let tracked_ids = schema
        .jobs
        .iter()
        .map(|job| job.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let untracked_jobs = aggregated
        .keys()
        .filter(|name| !tracked_ids.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let relevant_jobs = schema
        .jobs
        .iter()
        .filter(|job| match pipeline_product {
            "release-execution" => job.pipeline_product == "release-execution",
            "production-promotion" => job.pipeline_product == "production-promotion",
            _ => {
                job.pipeline_product != "release-execution"
                    && job.pipeline_product != "production-promotion"
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    apply_vti_selected_omissions(&relevant_jobs, &vti_metadata, &mut aggregated);

    let release_critical = pipeline_lane_progress(
        &relevant_jobs,
        &aggregated,
        "release-blocking",
        &pipeline.status,
    );
    let extended =
        pipeline_lane_progress(&relevant_jobs, &aggregated, "extended", &pipeline.status);
    let research =
        pipeline_lane_progress(&relevant_jobs, &aggregated, "research", &pipeline.status);
    let release_execution = pipeline_lane_progress(
        &relevant_jobs,
        &aggregated,
        "release-execution",
        &pipeline.status,
    );

    let mut blocking_failed = Vec::new();
    let mut blocking_pending = Vec::new();
    let mut non_blocking_failed = Vec::new();
    let mut non_blocking_pending = Vec::new();
    for job in &relevant_jobs {
        let state = aggregated.get(&job.id);
        let status = effective_job_status(state, &pipeline.status);
        if matches!(status, "success" | "skipped" | "omitted" | "vti-skipped") {
            continue;
        }
        let item = pipeline_item(job, state, status);
        if matches!(status, "failed" | "canceled") {
            if job.release_blocking {
                blocking_failed.push(item);
            } else {
                non_blocking_failed.push(item);
            }
        } else if job.release_blocking {
            blocking_pending.push(item);
        } else {
            non_blocking_pending.push(item);
        }
    }

    let mut incomplete_milestones = Vec::new();
    for milestone in &schema.milestones {
        if milestone.pipeline_product != pipeline_product {
            continue;
        }
        let mut statuses = Vec::new();
        let mut failed = false;
        let mut incomplete = Vec::new();
        for job in &milestone.jobs {
            let status = effective_job_status(aggregated.get(job), &pipeline.status);
            statuses.push(status.to_string());
            if !matches!(status, "success" | "skipped" | "omitted" | "vti-skipped") {
                incomplete.push(job.clone());
            }
            if matches!(status, "failed" | "canceled") {
                failed = true;
            }
        }
        if incomplete.is_empty() {
            continue;
        }
        let status = if failed { "failed" } else { "pending" };
        incomplete_milestones.push(PipelineExplainMilestone {
            id: milestone.id.clone(),
            title: milestone.title.clone(),
            status: status.to_string(),
            lane: milestone.lane.clone(),
            jobs: milestone.jobs.clone(),
            incomplete_jobs: incomplete,
        });
    }

    let release_candidate_materialized = pipeline_product != "main-candidate"
        || aggregated_materializes_release_candidate(&aggregated);

    let current_blocker = if !release_candidate_materialized {
        Some("release candidate jobs omitted by VTI".to_string())
    } else if let Some(item) = blocking_failed.first() {
        Some(format!("{} failed on {}", item.id, item.runner_pool))
    } else {
        incomplete_milestones.first().map(|milestone| {
            format!(
                "{} pending: {}",
                milestone.title,
                milestone.incomplete_jobs.join(", ")
            )
        })
    };
    let release_eligible =
        release_candidate_materialized && blocking_failed.is_empty() && blocking_pending.is_empty();

    Ok(PipelineExplainReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id,
        pipeline_id,
        pipeline_sha: pipeline.sha,
        pipeline_ref: pipeline.ref_name,
        pipeline_status: pipeline.status,
        release_critical,
        extended,
        research,
        release_execution,
        current_blocker,
        release_eligible,
        blocking_failed,
        blocking_pending,
        non_blocking_failed,
        non_blocking_pending,
        incomplete_milestones,
        untracked_jobs,
    })
}

pub async fn trigger_production_promotion(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    version: Option<String>,
) -> Result<i64> {
    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: None,
            limit: 20,
        },
    )
    .await?;
    let view = report
        .recent
        .iter()
        .find(|view| {
            version
                .as_deref()
                .map(|wanted| view.attempt.version == wanted)
                .unwrap_or(true)
        })
        .context("no release attempt found for production promotion")?;
    if view.canary_state != "e2e-passed" {
        return Err(ReleaseError::CanaryGateRejected {
            version: view.attempt.version.clone(),
            state: view.canary_state.clone(),
        }
        .into());
    }

    // Phase 4: Admission Control Enforcement - C Artifact Handoff validation.
    let release_root = release_dir(&view.attempt.version);
    let c_handoff_path = release_root.join("rendered/c-handoff.json");
    let c_validation_path = release_root.join("c-validation.json");

    if !c_handoff_path.exists() {
        return Err(ReleaseError::MissingHandoff {
            version: view.attempt.version.clone(),
            path: c_handoff_path,
        }
        .into());
    }
    if !c_validation_path.exists() {
        return Err(ReleaseError::MissingValidation {
            version: view.attempt.version.clone(),
            path: c_validation_path,
        }
        .into());
    }

    let sha = view.attempt.sha.clone();
    if let Some(existing_id) =
        production_promotion_pipeline_id(client, project_id, ref_name, &sha).await?
    {
        info!(
            project_id,
            pipeline_id = existing_id,
            ref_name = %ref_name,
            sha = %sha,
            version = %view.attempt.version,
            "production-promotion pipeline already exists"
        );
        return Ok(existing_id);
    }

    crate::cache::ensure_root_disk_headroom(
        crate::cache::ROOT_DISK_HEADROOM_MIN_FREE_BYTES,
        "production promotion",
    )
    .await?;

    let release_version = view.attempt.version.clone();
    let release_pipeline_id_str = view
        .attempt
        .release_pipeline_id
        .map(|id| id.to_string())
        .unwrap_or_default();
    let mut trigger_vars = vec![
        ("CI_PIPELINE_PRODUCT", "production-promotion"),
        ("JERYU_PROD_APPROVED", "1"),
        ("JERYU_RELEASE_SHA", sha.as_str()),
        ("JERYU_RELEASE_VERSION", release_version.as_str()),
    ];
    if !release_pipeline_id_str.is_empty() {
        trigger_vars.push((
            "JERYU_RELEASE_PIPELINE_ID",
            release_pipeline_id_str.as_str(),
        ));
        trigger_vars.push((
            "JERYU_RELEASE_PIPELINE_ID",
            release_pipeline_id_str.as_str(),
        ));
    }
    let pipeline_id = client
        .trigger_pipeline(project_id, ref_name, trigger_vars)
        .await?;

    db.attach_production_pipeline(project_id, ref_name, &sha, pipeline_id, "created")
        .await?;

    let _ = db
        .upsert_tracked_pipeline(&crate::state::TrackedPipeline {
            pipeline_id,
            project_id,
            ref_name: ref_name.to_string(),
            sha: sha.clone(),
            status: "created".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
        .await;

    Ok(pipeline_id)
}

pub async fn maybe_trigger_production_promotion(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    sha: Option<&str>,
    version: Option<&str>,
) -> Result<Option<i64>> {
    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: sha.map(ToOwned::to_owned),
            limit: 20,
        },
    )
    .await?;

    let matches_requested = |view: &&ReleaseAttemptView| {
        version
            .map(|wanted| view.attempt.version == wanted)
            .unwrap_or(true)
            && sha.map(|wanted| view.attempt.sha == wanted).unwrap_or(true)
    };
    let selected = if version.is_some() || sha.is_some() {
        report.recent.iter().find(matches_requested)
    } else {
        report.latest.as_ref()
    };
    let Some(view) = selected else {
        return Ok(None);
    };

    // Sync CI artifacts to local disk if release pipeline succeeded and gate files are missing.
    let gate_files_before_sync = canary_gate_files(&view.attempt.version);
    if view.attempt.release_pipeline_status.as_deref() == Some("success")
        && let Some(release_pipeline_id) = view.attempt.release_pipeline_id
        && (!gate_files_before_sync.e2e
            || !gate_files_before_sync.handoff
            || !gate_files_before_sync.validation
            || !release_dir(&view.attempt.version).join("release.json").is_file()
            || !release_dir(&view.attempt.version)
                .join("release-contract.json")
                .is_file())
        && let Err(err) = sync_canary_artifacts(
            client,
            project_id,
            release_pipeline_id,
            &view.attempt.version,
        )
        .await
    {
        warn!(
            project_id,
            version = %view.attempt.version,
            error = %err,
            "artifact sync failed; production promotion may be delayed"
        );
    }

    // Re-evaluate gate file presence after potential artifact sync.
    let gate_files = canary_gate_files(&view.attempt.version);
    let gate_files_ok = gate_files.promotion_ready();
    let identity_ok = release_identity_ok(&view.attempt.version, &view.attempt.sha);

    if !gate_files_ok
        || !identity_ok
        || view.attempt.release_pipeline_status.as_deref() != Some("success")
        || gate_prod_promotion_path(&view.attempt.version).is_file()
    {
        return Ok(None);
    }

    if view.attempt.canary_status != "passed" {
        db.finish_release_canary(
            project_id,
            ref_name,
            &view.attempt.sha,
            "passed",
            Some("required canary gate evidence synced from release-execution pipeline"),
        )
        .await?;
    }

    if let Some(existing_id) =
        production_promotion_pipeline_id(client, project_id, ref_name, &view.attempt.sha).await?
    {
        db.attach_production_pipeline(
            project_id,
            ref_name,
            &view.attempt.sha,
            existing_id,
            "running",
        )
        .await?;
        return Ok(Some(existing_id));
    }

    let pipeline_id = trigger_production_promotion(
        db,
        client,
        project_id,
        ref_name,
        Some(view.attempt.version.clone()),
    )
    .await?;
    Ok(Some(pipeline_id))
}

#[cfg(test)]
fn should_trigger_production_promotion_with_gate(
    view: &ReleaseAttemptView,
    prod_gate_exists: bool,
) -> bool {
    view.canary_state == "e2e-passed"
        && view.release_identity_ok
        && view.has_remote_gate
        && view.has_telemetry_gate
        && view.has_e2e_gate
        && !prod_gate_exists
}

async fn production_promotion_pipeline_id(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    sha: &str,
) -> Result<Option<i64>> {
    for pipeline in client
        .list_pipelines(project_id, Some(ref_name))
        .await?
        .into_iter()
    {
        if !pipeline_matches_release_sha(client, project_id, pipeline.id, &pipeline.sha, sha)
            .await?
        {
            continue;
        }
        let jobs = aggregate_pipeline_jobs(
            client
                .list_pipeline_jobs_with_downstream(project_id, pipeline.id)
                .await?,
        );
        let Some(job) = jobs.get("promote-production-final") else {
            continue;
        };
        if matches!(
            job.status.as_str(),
            "created" | "pending" | "running" | "success"
        ) {
            return Ok(Some(pipeline.id));
        }
    }
    Ok(None)
}

async fn pipeline_matches_release_sha(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    pipeline_sha: &str,
    release_sha: &str,
) -> Result<bool> {
    if pipeline_sha == release_sha {
        return Ok(true);
    }
    match client
        .list_pipeline_variables(project_id, pipeline_id)
        .await
    {
        Ok(variables) => Ok(variables.iter().any(|variable| {
            matches!(variable.key.as_str(), "JERYU_RELEASE_SHA") && variable.value == release_sha
        })),
        Err(err) => {
            warn!(
                project_id,
                pipeline_id,
                error = %err,
                "could not inspect pipeline variables while checking production promotion"
            );
            Ok(false)
        }
    }
}

pub fn render_pipeline_explain_text(report: &PipelineExplainReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(out, "━━━ jeryu pipeline explain ━━━");
    let _ = writeln!(out, "  Pipeline:          {}", report.pipeline_id);
    let _ = writeln!(
        out,
        "  Ref/SHA:           {} / {}",
        report.pipeline_ref, report.pipeline_sha
    );
    let _ = writeln!(out, "  Status:            {}", report.pipeline_status);
    let _ = writeln!(out, "  Release eligible:  {}", report.release_eligible);
    let _ = writeln!(
        out,
        "  Current blocker:   {}",
        report.current_blocker.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "  Lane progress:");
    let _ = writeln!(
        out,
        "    Release-critical: {}/{} ({:.1}%)",
        report.release_critical.passed,
        report.release_critical.total,
        report.release_critical.percent
    );
    let _ = writeln!(
        out,
        "    Extended:         {}/{} ({:.1}%)",
        report.extended.passed, report.extended.total, report.extended.percent
    );
    let _ = writeln!(
        out,
        "    Research:         {}/{} ({:.1}%)",
        report.research.passed, report.research.total, report.research.percent
    );
    if report.release_execution.total > 0 {
        let _ = writeln!(
            out,
            "    Release execution: {}/{} ({:.1}%)",
            report.release_execution.passed,
            report.release_execution.total,
            report.release_execution.percent
        );
    }
    if !report.blocking_failed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Blocking failed:");
        for item in &report.blocking_failed {
            let _ = writeln!(
                out,
                "    - {} [{} / {} / {}]",
                item.id,
                item.runner_pool,
                item.kind,
                display_job_status(&item.status)
            );
        }
    }
    if !report.blocking_pending.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Blocking pending:");
        for item in &report.blocking_pending {
            let _ = writeln!(
                out,
                "    - {} [{} / {} / {}]",
                item.id,
                item.runner_pool,
                item.kind,
                display_job_status(&item.status)
            );
        }
    }
    if !report.non_blocking_failed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Non-blocking failed:");
        for item in &report.non_blocking_failed {
            let _ = writeln!(
                out,
                "    - {} [{} / {} / {}]",
                item.id,
                item.runner_pool,
                item.kind,
                display_job_status(&item.status)
            );
        }
    }
    if !report.incomplete_milestones.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Incomplete milestones:");
        for milestone in &report.incomplete_milestones {
            let _ = writeln!(
                out,
                "    - {} [{}] :: {}",
                milestone.title,
                milestone.status,
                milestone.incomplete_jobs.join(", ")
            );
        }
    }
    if !report.untracked_jobs.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Untracked pipeline jobs:");
        for job in &report.untracked_jobs {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    out
}

pub async fn build_pipeline_doctor_report(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<PipelineDoctorReport> {
    let root = repo_root();
    let schema = load_ci_schema(&root).await?;
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    let historical_bottlenecks = match Db::open().await {
        Ok(db) => db
            .ci_job_bottlenecks(project_id, Some(&pipeline.ref_name), 500)
            .await
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let schema_pools = schema
        .jobs
        .iter()
        .map(|job| (job.id.clone(), job.runner_pool.clone()))
        .collect::<HashMap<_, _>>();

    let mut doctor_jobs = Vec::new();
    for job in jobs {
        if !matches!(
            job.status.as_str(),
            "running" | "pending" | "created" | "waiting_for_resource" | "preparing"
        ) {
            continue;
        }
        let canonical_name = canonical_job_name(&job.name);
        let runner_pool = schema_pools
            .get(&canonical_name)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let historical = historical_bottlenecks
            .iter()
            .filter(|row| row.job_name == canonical_name)
            .max_by_key(|row| {
                (
                    row.runner_pool.as_deref() == Some(runner_pool.as_str()),
                    row.runs,
                )
            });
        let mut trace_bytes = None;
        let mut trace_tail = None;
        if job.status == "running"
            && let Ok(trace) = client.job_trace(project_id, job.id).await
        {
            trace_bytes = Some(trace.len());
            trace_tail = Some(
                trace
                    .lines()
                    .rev()
                    .filter(|line| !line.trim().is_empty())
                    .take(5)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        let duration = job.duration.or(job.queued_duration);
        let trace_empty = trace_bytes == Some(0) || trace_tail.as_deref().unwrap_or("").is_empty();
        let historical_avg_duration_secs = historical.map(|row| row.avg_duration_secs);
        let historical_max_duration_secs = historical.and_then(|row| row.max_duration_secs);
        let historical_runs = historical.map(|row| row.runs);
        let slow_factor = historical_avg_duration_secs
            .filter(|avg| *avg > 0.0)
            .and_then(|avg| duration.map(|current| current / avg));
        let queue_factor = historical_avg_duration_secs
            .filter(|avg| *avg > 0.0)
            .and_then(|avg| job.queued_duration.map(|queued| queued / avg));
        let stale_trace_suspected = job.status == "running"
            && trace_empty
            && (slow_factor.map(|factor| factor >= 1.5).unwrap_or(false)
                || duration.unwrap_or(0.0) > 900.0);
        let stuck_suspected = match job.status.as_str() {
            "running" => {
                stale_trace_suspected
                    || slow_factor
                        .map(|factor| factor >= 2.0)
                        .unwrap_or(duration.unwrap_or(0.0) > 600.0)
            }
            "pending" | "created" | "waiting_for_resource" | "preparing" => queue_factor
                .map(|factor| factor >= 2.0)
                .unwrap_or(job.queued_duration.unwrap_or(0.0) > 600.0),
            _ => false,
        };
        let recommendation = if stale_trace_suspected {
            let avg = historical_avg_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or_else(|| "n/a".to_string());
            let slow = slow_factor
                .map(|value| format!("{value:.2}x"))
                .unwrap_or_else(|| "n/a".to_string());
            "trace looks outdated relative to historical runtime; recycle the runner or retry after checking trace capture".to_string()
                + &format!(" (avg={}, slow={})", avg, slow)
        } else if stuck_suspected && job.status == "running" {
            "cancel/retry this job or recycle its runner; it is materially slower than historical timing".to_string()
        } else if stuck_suspected {
            "check runner capacity and tags for this pool; queue time is materially above historical timing".to_string()
        } else if job.status == "running" {
            "job is running; compare runtime against historical avg/max and inspect trace if it remains slow".to_string()
        } else {
            "waiting for eligible runner".to_string()
        };
        doctor_jobs.push(PipelineDoctorJob {
            id: job.id,
            name: job.name,
            canonical_name,
            status: job.status,
            stage: job.stage,
            runner_pool,
            runner: job.runner.and_then(|runner| runner.description),
            started_at: job.started_at,
            duration_secs: job.duration,
            queued_duration_secs: job.queued_duration,
            historical_avg_duration_secs,
            historical_max_duration_secs,
            historical_runs,
            slow_factor,
            queue_factor,
            trace_bytes,
            trace_tail,
            stuck_suspected,
            stale_trace_suspected,
            recommendation,
        });
    }
    let stuck_suspected = doctor_jobs
        .iter()
        .filter(|job| job.stuck_suspected)
        .cloned()
        .collect::<Vec<_>>();
    Ok(PipelineDoctorReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id,
        pipeline_id,
        pipeline_sha: pipeline.sha,
        pipeline_ref: pipeline.ref_name,
        pipeline_status: pipeline.status,
        jobs: doctor_jobs,
        stuck_suspected,
    })
}

pub fn render_pipeline_doctor_text(report: &PipelineDoctorReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(out, "━━━ jeryu pipeline doctor ━━━");
    let _ = writeln!(out, "  Pipeline: {}", report.pipeline_id);
    let _ = writeln!(
        out,
        "  Ref/SHA:  {} / {}",
        report.pipeline_ref, report.pipeline_sha
    );
    let _ = writeln!(out, "  Status:   {}", report.pipeline_status);
    let _ = writeln!(out, "  Active:   {}", report.jobs.len());
    let _ = writeln!(out, "  Suspect:  {}", report.stuck_suspected.len());
    if !report.jobs.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Active jobs:");
        for job in &report.jobs {
            let trace = job
                .trace_bytes
                .map(|bytes| format!("{bytes}b trace"))
                .unwrap_or_else(|| "trace n/a".to_string());
            let current = job
                .duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or_else(|| "-".to_string());
            let queue = job
                .queued_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or_else(|| "-".to_string());
            let avg = job
                .historical_avg_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or_else(|| "-".to_string());
            let max = job
                .historical_max_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or_else(|| "-".to_string());
            let slow = job
                .slow_factor
                .map(|value| format!("{value:.2}x"))
                .unwrap_or_else(|| "-".to_string());
            let queue_factor = job
                .queue_factor
                .map(|value| format!("{value:.2}x"))
                .unwrap_or_else(|| "-".to_string());
            let marker = if job.stuck_suspected { "!" } else { "-" };
            let _ = writeln!(
                out,
                "    {} {} #{} [{} / {} / {}] run={} avg={} max={} slow={} queue={} qslow={} trace={}",
                marker,
                job.canonical_name,
                job.id,
                job.runner_pool,
                job.stage,
                job.status,
                current,
                avg,
                max,
                slow,
                queue,
                queue_factor,
                trace
            );
            if job.stuck_suspected {
                if let Some(runs) = job.historical_runs {
                    let _ = writeln!(out, "      history: {} runs", runs);
                }
                let _ = writeln!(out, "      recommendation: {}", job.recommendation);
            }
            if job.stale_trace_suspected {
                let _ = writeln!(out, "      trace: outdated compared with historical timing");
            }
        }
    }
    out
}

pub async fn reconcile_release_for_ref(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<ReleaseStatusReport> {
    let Some(pipeline) =
        latest_release_candidate_pipeline_for_ref(client, project_id, ref_name).await?
    else {
        return build_release_status_report(
            db,
            ReleaseStatusQuery {
                project_id: Some(project_id),
                ref_name: Some(ref_name.to_string()),
                sha: None,
                limit: 5,
            },
        )
        .await;
    };

    let version = render_release_version(&pipeline.sha);
    let mut existing = db
        .get_release_attempt(project_id, ref_name, &pipeline.sha)
        .await?;
    if let Some(attempt) = existing.as_ref()
        && let Some(release_pipeline_id) = attempt.release_pipeline_id
    {
        let release_pipeline = client
            .get_pipeline(project_id, release_pipeline_id)
            .await
            .with_context(|| {
                format!("refresh release pipeline {release_pipeline_id} before reconcile")
            })?;
        if attempt.release_pipeline_status.as_deref() != Some(release_pipeline.status.as_str()) {
            existing = db
                .update_release_pipeline_status(release_pipeline_id, &release_pipeline.status)
                .await?;
        }
        if matches!(release_pipeline.status.as_str(), "failed" | "canceled")
            && existing
                .as_ref()
                .map(|attempt| attempt.canary_status.as_str())
                == Some("running")
        {
            let note = format!(
                "release-execution pipeline {release_pipeline_id} ended with status {}",
                release_pipeline.status
            );
            db.finish_release_canary(project_id, ref_name, &pipeline.sha, "failed", Some(&note))
                .await?;
            existing = db
                .get_release_attempt(project_id, ref_name, &pipeline.sha)
                .await?;
        }
    }
    let mut existing_canary_status = existing
        .as_ref()
        .map(|attempt| attempt.canary_status.as_str())
        .unwrap_or("pending");
    if existing_canary_status == "passed"
        && !has_complete_canary_evidence(&release_evidence(&version, &pipeline.sha)?)
    {
        let note = "release-execution pipeline ended without required canary gate evidence";
        db.finish_release_canary(project_id, ref_name, &pipeline.sha, "failed", Some(note))
            .await?;
        existing_canary_status = "failed";
        existing = db
            .get_release_attempt(project_id, ref_name, &pipeline.sha)
            .await?;
    }
    let needs_upsert = existing
        .as_ref()
        .map(|attempt| {
            attempt.upstream_pipeline_id != Some(pipeline.id)
                || attempt.upstream_status != "success"
                || attempt.version != version
        })
        .unwrap_or(true);
    if needs_upsert {
        db.upsert_release_attempt(
            project_id,
            ref_name,
            &pipeline.sha,
            &version,
            Some(pipeline.id),
            "success",
            existing_canary_status,
        )
        .await?;
    }

    if !matches!(existing_canary_status, "running" | "passed" | "skipped") {
        launch_canary_for_green_pipeline(
            db,
            client,
            project_id,
            ref_name,
            &pipeline.sha,
            pipeline.id,
        )
        .await?;
    }

    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: Some(pipeline.sha),
            limit: 5,
        },
    )
    .await?;

    if let Some(latest) = report.latest.as_ref()
        && maybe_trigger_production_promotion(
            db,
            client,
            project_id,
            ref_name,
            Some(&latest.attempt.sha),
            Some(&latest.attempt.version),
        )
        .await?
        .is_some()
    {
        return build_release_status_report(
            db,
            ReleaseStatusQuery {
                project_id: Some(project_id),
                ref_name: Some(ref_name.to_string()),
                sha: Some(latest.attempt.sha.clone()),
                limit: 5,
            },
        )
        .await;
    }

    Ok(report)
}

async fn pipeline_has_release_execution_jobs(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<bool> {
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    Ok(jobs.iter().any(|job| {
        matches!(
            job.name.as_str(),
            "deploy-canary-final" | "report-testing-punchlist" | "promote-production-final"
        )
    }))
}

#[derive(Debug, Clone)]
struct UpstreamImageHandoff {
    artifact_pipeline_id: i64,
    build_job_id: i64,
    image_ref: String,
}

fn parse_image_env(raw: &str) -> HashMap<String, String> {
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

async fn upstream_image_handoff(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<Option<UpstreamImageHandoff>> {
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    let Some(job) = jobs
        .iter()
        .find(|job| job.name == "build-enclave-server" && job.status == "success")
    else {
        return Ok(None);
    };
    let artifact_pipeline_id = job.pipeline_id.unwrap_or(pipeline_id);
    let artifact_path = format!("ops/releases/{artifact_pipeline_id}/image.env");
    let raw = match client
        .job_artifact_file(project_id, job.id, &artifact_path)
        .await
    {
        Ok(raw) => raw,
        Err(err) => {
            warn!(
                project_id,
                pipeline_id,
                artifact_pipeline_id,
                job_id = job.id,
                error = %err,
                "could not read upstream image handoff artifact; canary will rebuild"
            );
            return Ok(None);
        }
    };
    let env = parse_image_env(&raw);
    if env
        .get("VEOX_PUBLIC_SURFACE_IMAGE_HANDOFF")
        .map(|value| value == "registry")
        != Some(true)
    {
        return Ok(None);
    }
    let Some(image_ref) = env
        .get("VEOX_PUBLIC_SURFACE_IMAGE")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(UpstreamImageHandoff {
        artifact_pipeline_id,
        build_job_id: job.id,
        image_ref,
    }))
}

async fn release_impacting_change(sha: &str) -> Result<bool> {
    let root = repo_root();
    let base_ref = format!("{sha}^");
    let output = Command::new("cargo")
        .current_dir(&root)
        .args([
            "run",
            "-p",
            "veox-testctl",
            "--",
            "ci-impact",
            "--base",
            base_ref.as_str(),
            "--head",
            sha,
            "--json",
        ])
        .output()
        .await
        .with_context(|| format!("run ci-impact for {sha}"))?;
    if !output.status.success() {
        warn!(
            sha = %sha,
            stderr = %String::from_utf8_lossy(&output.stderr),
            "ci-impact failed; treating change as release-impacting"
        );
        return Ok(true);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).with_context(|| "parse ci-impact json output")?;
    Ok(value
        .get("release_impacting")
        .and_then(|value| value.as_bool())
        .unwrap_or(true))
}

pub async fn launch_canary_for_green_pipeline(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    sha: &str,
    pipeline_id: i64,
) -> Result<()> {
    let ref_name = ref_name.trim();
    if ref_name != "main" {
        return Ok(());
    }

    let version = render_release_version(sha);
    if pipeline_has_release_execution_jobs(client, project_id, pipeline_id).await? {
        info!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            "pipeline is already a release-execution pipeline; skipping canary trigger"
        );
        return Ok(());
    }

    let Some(latest) =
        latest_release_candidate_pipeline_for_ref(client, project_id, ref_name).await?
    else {
        return Ok(());
    };
    if latest.id != pipeline_id || latest.sha != sha {
        info!(
            project_id,
            pipeline_id,
            latest_pipeline_id = latest.id,
            latest_status = %latest.status,
            ref_name = %ref_name,
            sha = %sha,
            "upstream pipeline is no longer the latest successful ref state; skipping canary trigger"
        );
        return Ok(());
    }

    let explain = build_pipeline_explain_report(client, project_id, pipeline_id).await?;
    let extended_green =
        explain.extended.total == 0 || explain.extended.passed == explain.extended.total;
    if !explain.release_eligible || !extended_green {
        let note = format!(
            "full-build gate not satisfied: release_eligible={} extended={}/{} blocker={}",
            explain.release_eligible,
            explain.extended.passed,
            explain.extended.total,
            explain.current_blocker.as_deref().unwrap_or("none")
        );
        db.finish_release_canary(project_id, ref_name, sha, "blocked", Some(&note))
            .await?;
        warn!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            note = %note,
            "refusing automatic canary for incomplete full build"
        );
        return Ok(());
    }

    if !release_impacting_change(sha).await? {
        db.upsert_release_attempt(
            project_id,
            ref_name,
            sha,
            &version,
            Some(pipeline_id),
            "success",
            "skipped",
        )
        .await?;
        db.finish_release_canary(
            project_id,
            ref_name,
            sha,
            "skipped",
            Some("change-impact policy classified this commit as non-release-impacting"),
        )
        .await?;
        info!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            version = %version,
            "release-impact policy skipped automatic canary"
        );
        return Ok(());
    }

    let claimed = db
        .claim_release_canary(project_id, ref_name, sha, &version, Some(pipeline_id))
        .await?;
    if !claimed {
        info!(
            project_id,
            pipeline_id,
            ref_name = %ref_name,
            sha = %sha,
            version = %version,
            "release candidate already claimed or completed"
        );
        return Ok(());
    }

    info!(
        project_id,
        pipeline_id,
        ref_name = %ref_name,
        sha = %sha,
        version = %version,
        "upstream pipeline green; launching canary"
    );

    // Preflight: verify SSH/Vault/registry/disk before burning a pipeline slot.
    let pf = release_preflight(None).await;
    if !pf.ok {
        let blockers: Vec<String> = pf
            .blockers
            .iter()
            .map(|b| format!("[{}] {}", b.code, b.detail))
            .collect();
        let note = format!("release preflight failed: {}", blockers.join("; "));
        db.finish_release_canary(project_id, ref_name, sha, "blocked", Some(&note))
            .await?;
        warn!(project_id, pipeline_id, ref_name = %ref_name, sha = %sha, note = %note, "preflight blocked canary launch");
        return Ok(());
    }

    let image_handoff = upstream_image_handoff(client, project_id, pipeline_id).await?;
    let upstream_artifact_pipeline_id = image_handoff
        .as_ref()
        .map(|handoff| handoff.artifact_pipeline_id)
        .unwrap_or(pipeline_id);
    let upstream_pipeline_id = upstream_artifact_pipeline_id.to_string();
    let upstream_build_job_id = image_handoff
        .as_ref()
        .map(|handoff| handoff.build_job_id.to_string());
    let upstream_enclave_image_ref = image_handoff
        .as_ref()
        .map(|handoff| handoff.image_ref.clone());
    if let Some(handoff) = &image_handoff {
        info!(
            project_id,
            pipeline_id,
            artifact_pipeline_id = handoff.artifact_pipeline_id,
            build_job_id = handoff.build_job_id,
            image_ref = %handoff.image_ref,
            "upstream registry image handoff found; canary will skip enclave rebuild"
        );
    }
    let release_pipeline_id = match client
        .trigger_pipeline(project_id, ref_name, {
            let mut variables = vec![
                ("CI_PIPELINE_PRODUCT", "release-execution"),
                ("JERYU_CANARY_APPROVED", "1"),
                ("JERYU_UPSTREAM_PIPELINE_ID", upstream_pipeline_id.as_str()),
                ("JERYU_RELEASE_SHA", sha),
                ("JERYU_RELEASE_VERSION", version.as_str()),
            ];
            if let Some(job_id) = upstream_build_job_id.as_deref() {
                variables.push(("JERYU_UPSTREAM_BUILD_JOB_ID", job_id));
            }
            if let Some(image_ref) = upstream_enclave_image_ref.as_deref() {
                variables.push(("VEOX_PUBLISH_ENCLAVE_REF", image_ref));
            }
            variables
        })
        .await
    {
        Ok(pipeline_id) => pipeline_id,
        Err(err) => {
            let note = format!("release-execution trigger failed before attach: {err}");
            db.finish_release_canary(project_id, ref_name, sha, "failed", Some(&note))
                .await?;
            return Err(err)
                .with_context(|| format!("trigger release-execution pipeline for {sha}"));
        }
    };

    let _ = db
        .upsert_tracked_pipeline(&crate::state::TrackedPipeline {
            pipeline_id: release_pipeline_id,
            project_id,
            ref_name: ref_name.to_string(),
            sha: sha.to_string(),
            status: "created".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
        .await;

    db.attach_release_pipeline(project_id, ref_name, sha, release_pipeline_id, "pending")
        .await?;
    info!(
        project_id,
        upstream_pipeline_id = pipeline_id,
        upstream_artifact_pipeline_id,
        release_pipeline_id,
        ref_name = %ref_name,
        sha = %sha,
        version = %version,
        "triggered release-execution canary pipeline"
    );
    // Write release-lock.json before triggering so CI jobs can assert identity.
    let lock = ReleaseLock {
        schema: 1,
        release_version: version.clone(),
        product_sha: sha.to_string(),
        certifying_pipeline_id: pipeline_id,
        upstream_pipeline_id: upstream_artifact_pipeline_id,
        build_job_id: image_handoff.as_ref().map(|h| h.build_job_id),
        image_ref: upstream_enclave_image_ref.clone(),
        release_tool_sha: option_env!("VERGEN_GIT_SHA")
            .unwrap_or("unknown")
            .to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    write_release_lock(&version, &lock);

    Ok(())
}

// ── Release lock ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseLock {
    pub schema: u32,
    pub release_version: String,
    pub product_sha: String,
    pub certifying_pipeline_id: i64,
    pub upstream_pipeline_id: i64,
    pub build_job_id: Option<i64>,
    pub image_ref: Option<String>,
    pub release_tool_sha: String,
    pub created_at: String,
}

fn release_lock_path(version: &str) -> PathBuf {
    release_dir(version).join("release-lock.json")
}

fn write_release_lock(version: &str, lock: &ReleaseLock) {
    let path = release_lock_path(version);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(lock) {
        Ok(json) => {
            if let Err(e) = fs::write(&path, json) {
                warn!(version, error = %e, "failed to write release-lock.json");
            } else {
                info!(version, path = %path.display(), "wrote release-lock.json");
            }
        }
        Err(e) => warn!(version, error = %e, "failed to serialize release-lock.json"),
    }
}

// ── Release preflight ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightBlocker {
    pub code: String,
    pub component: String,
    pub detail: String,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightReport {
    pub ok: bool,
    pub blockers: Vec<PreflightBlocker>,
    pub checks: std::collections::HashMap<String, String>,
    pub generated_at: String,
}

pub async fn release_preflight(ssh_host: Option<&str>) -> PreflightReport {
    let mut blockers = Vec::new();
    let mut checks = std::collections::HashMap::new();
    let target = ssh_host.unwrap_or("atomicsoul");

    // SSH check
    let ssh_ok = tokio::process::Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=no",
            target,
            "echo",
            "ci-preflight-ok",
        ])
        .output()
        .await
        .map(|o| {
            o.status.success() && String::from_utf8_lossy(&o.stdout).contains("ci-preflight-ok")
        })
        .unwrap_or(false);
    checks.insert(
        "ssh".to_string(),
        if ssh_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !ssh_ok {
        blockers.push(PreflightBlocker {
            code: "SSH_UNREACHABLE".to_string(),
            component: "canary-target".to_string(),
            detail: format!("SSH to {target} failed (ConnectTimeout=5)"),
            recommended_action: format!(
                "verify {target} is powered on and reachable from this host"
            ),
        });
    }

    // Vault check
    let vault_port = crate::config::VAULT_HTTP_PORT;
    let vault_url = format!("http://127.0.0.1:{vault_port}/v1/sys/health");
    let vault_ok = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(client) => client
            .get(&vault_url)
            .send()
            .await
            .map(|r| r.status().as_u16() < 500)
            .unwrap_or(false),
        Err(_) => false,
    };
    checks.insert(
        "vault".to_string(),
        if vault_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !vault_ok {
        blockers.push(PreflightBlocker {
            code: "VAULT_UNREACHABLE".to_string(),
            component: "vault".to_string(),
            detail: format!("Vault health check failed at {vault_url}"),
            recommended_action: "run: jeryu cache doctor; check vault container is running"
                .to_string(),
        });
    }

    // Registry check (TCP connect to local registry mirror)
    let registry_port = crate::settings::get().cache.registry_port;
    let registry_ok = tokio::net::TcpStream::connect(format!("127.0.0.1:{registry_port}"))
        .await
        .is_ok();
    checks.insert(
        "registry".to_string(),
        if registry_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !registry_ok {
        blockers.push(PreflightBlocker {
            code: "REGISTRY_UNREACHABLE".to_string(),
            component: "registry-mirror".to_string(),
            detail: format!("registry mirror TCP connect to 127.0.0.1:{registry_port} failed"),
            recommended_action: "run: jeryu serve (starts registry mirror)".to_string(),
        });
    }

    // Disk check
    const DISK_EMERGENCY_FREE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
    const DISK_CRITICAL_FREE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
    const DISK_WARNING_FREE_BYTES: u64 = 75 * 1024 * 1024 * 1024;
    let disk_status = match crate::cache::df_usage("/").await {
        Ok(usage) => {
            if usage.available_bytes < DISK_EMERGENCY_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "emergency ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                blockers.push(PreflightBlocker {
                    code: "DISK_EMERGENCY".to_string(),
                    component: "host-disk".to_string(),
                    detail: format!(
                        "root disk only has {} free",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                    recommended_action: "run: jeryu cache status --json; then jeryu cache gc --json --keep-active-managers=false --max-cache-gb 20".to_string(),
                });
                false
            } else if usage.available_bytes < DISK_CRITICAL_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "critical ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                blockers.push(PreflightBlocker {
                    code: "DISK_CRITICAL".to_string(),
                    component: "host-disk".to_string(),
                    detail: format!(
                        "root disk only has {} free",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                    recommended_action: "run: jeryu cache status --json; then jeryu cache gc --dry-run --json --older-than 12h --max-cache-gb 20".to_string(),
                });
                false
            } else if usage.available_bytes < DISK_WARNING_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "warning ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                true
            } else {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "ok ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                true
            }
        }
        Err(_) => {
            checks.insert("disk".to_string(), "unknown".to_string());
            true
        }
    };
    let _ = disk_status; // disk warning doesn't block

    PreflightReport {
        ok: blockers.is_empty(),
        blockers,
        checks,
        generated_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ── Release doctor ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorBlocker {
    pub code: String,
    pub gate: Option<String>,
    pub detail: String,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub version: String,
    pub product_sha: String,
    pub next_action: String,
    pub blockers: Vec<DoctorBlocker>,
    pub preflight: std::collections::HashMap<String, String>,
    pub gates: std::collections::HashMap<String, bool>,
    pub canary_complete: bool,
    pub prod_complete: bool,
    pub safe_to_reconcile: bool,
    pub generated_at: String,
}

pub async fn release_doctor(version: &str, run_preflight: bool) -> DoctorReport {
    let mut blockers = Vec::new();
    let mut gates = std::collections::HashMap::new();

    // Check gate files
    let gate_files = canary_gate_files(version);
    let gate_prod = gate_prod_promotion_path(version).exists();
    gates.insert("remote".to_string(), gate_files.remote);
    gates.insert("telemetry".to_string(), gate_files.telemetry);
    gates.insert("e2e".to_string(), gate_files.e2e);
    gates.insert("prod".to_string(), gate_prod);
    gates.insert("c_validation".to_string(), gate_files.validation);

    let canary_complete = gate_files.canary_complete();
    let prod_complete = gate_prod;

    // Check missing gates
    for (name, present, path) in [
        (
            "gate-remote-canary",
            gate_files.remote,
            gate_remote_canary_path(version),
        ),
        (
            "gate-canary-telemetry",
            gate_files.telemetry,
            gate_canary_telemetry_path(version),
        ),
        ("gate-canary-e2e", gate_files.e2e, gate_canary_e2e_path(version)),
        ("c-validation", gate_files.validation, c_validation_path(version)),
    ] {
        if !present {
            blockers.push(DoctorBlocker {
                code: "GATE_MISSING".to_string(),
                gate: Some(name.to_string()),
                detail: format!("{} not found at {}", name, path.display()),
                recommended_action: "run: jeryu release reconcile (triggers new canary attempt)"
                    .to_string(),
            });
        }
    }

    // Check release-lock
    let lock_path = release_lock_path(version);
    if !lock_path.exists() {
        blockers.push(DoctorBlocker {
            code: "LOCK_MISSING".to_string(),
            gate: None,
            detail: format!("release-lock.json not found at {}", lock_path.display()),
            recommended_action:
                "run: jeryu release reconcile (generates lock on next canary trigger)".to_string(),
        });
    }

    // Run preflight checks
    let preflight_checks = if run_preflight {
        let pf = release_preflight(None).await;
        for b in &pf.blockers {
            blockers.push(DoctorBlocker {
                code: b.code.clone(),
                gate: None,
                detail: b.detail.clone(),
                recommended_action: b.recommended_action.clone(),
            });
        }
        pf.checks
    } else {
        let mut m = std::collections::HashMap::new();
        m.insert("ssh".to_string(), "not-checked".to_string());
        m.insert("vault".to_string(), "not-checked".to_string());
        m.insert("registry".to_string(), "not-checked".to_string());
        m.insert("disk".to_string(), "not-checked".to_string());
        m
    };

    // Determine next action
    let next_action = if prod_complete {
        "done"
    } else if canary_complete {
        "run_production_promotion"
    } else if !blockers.iter().any(|b| {
        matches!(
            b.code.as_str(),
            "SSH_UNREACHABLE" | "VAULT_UNREACHABLE" | "REGISTRY_UNREACHABLE" | "DISK_EMERGENCY"
        )
    }) {
        "run_canary_retry"
    } else {
        "fix_preflight_blockers"
    };

    // Read product_sha from lock if available
    let product_sha = fs::read_to_string(lock_path)
        .ok()
        .and_then(|s| serde_json::from_str::<ReleaseLock>(&s).ok())
        .map(|l| l.product_sha)
        .unwrap_or_else(|| "unknown".to_string());

    DoctorReport {
        version: version.to_string(),
        product_sha,
        next_action: next_action.to_string(),
        blockers,
        preflight: preflight_checks,
        gates,
        canary_complete,
        prod_complete,
        safe_to_reconcile: next_action != "fix_preflight_blockers",
        generated_at: chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn job(name: &str, status: &str, allow_failure: bool) -> Job {
        Job {
            id: 1,
            name: name.to_string(),
            status: status.to_string(),
            stage: "test".to_string(),
            allow_failure,
            pipeline_id: None,
            ref_name: Some("main".to_string()),
            web_url: None,
            queued_duration: None,
            duration: None,
            started_at: None,
            finished_at: None,
            runner: None,
        }
    }

    #[test]
    fn version_uses_sha_prefix() {
        assert_eq!(
            render_release_version("abcdef1234567890"),
            "ci-abcdef123456"
        );
    }

    #[test]
    fn status_text_includes_state_paths() {
        let attempt = ReleaseAttempt {
            id: 1,
            project_id: 2,
            ref_name: "main".into(),
            sha: "abcdef1234567890".into(),
            version: "ci-abcdef123456".into(),
            upstream_pipeline_id: Some(77),
            upstream_status: "success".into(),
            release_pipeline_id: Some(88),
            release_pipeline_status: Some("running".into()),
            production_pipeline_id: None,
            production_pipeline_status: None,
            canary_status: "running".into(),
            canary_started_at: Some("2026-04-16T00:00:00Z".into()),
            canary_finished_at: None,
            canary_note: Some("launching".into()),
            created_at: "2026-04-16T00:00:00Z".into(),
            updated_at: "2026-04-16T00:00:01Z".into(),
        };
        let report = ReleaseStatusReport {
            generated_at: "2026-04-16T00:00:02Z".into(),
            project_id: Some(2),
            ref_name: Some("main".into()),
            sha: None,
            limit: 5,
            total_attempts: 1,
            latest: Some(view_attempt(attempt.clone()).expect("view attempt")),
            recent: vec![view_attempt(attempt).expect("view attempt")],
        };
        let text = render_release_status_text(&report);
        assert!(text.contains("jeryu release status"));
        assert!(text.contains("deploy-canary-c-state.json"));
        assert!(text.contains("Phase:"));
        assert!(text.contains("Gates:"));
        assert!(text.contains("telemetry_diag="));
    }

    #[test]
    fn auto_promotion_requires_complete_canary_evidence() {
        let attempt = ReleaseAttempt {
            id: 1,
            project_id: 2,
            ref_name: "main".into(),
            sha: "abcdef1234567890".into(),
            version: "ci-abcdef123456".into(),
            upstream_pipeline_id: Some(77),
            upstream_status: "success".into(),
            release_pipeline_id: Some(88),
            release_pipeline_status: Some("success".into()),
            production_pipeline_id: None,
            production_pipeline_status: None,
            canary_status: "passed".into(),
            canary_started_at: Some("2026-04-16T00:00:00Z".into()),
            canary_finished_at: Some("2026-04-16T00:10:00Z".into()),
            canary_note: Some("done".into()),
            created_at: "2026-04-16T00:00:00Z".into(),
            updated_at: "2026-04-16T00:10:01Z".into(),
        };
        let view = ReleaseAttemptView {
            attempt,
            release_dir: "/tmp/release".into(),
            canary_state_path: "/tmp/release/deploy-canary-c-state.json".into(),
            gate_remote_canary_path: "/tmp/release/gate-remote-canary.json".into(),
            gate_canary_e2e_path: "/tmp/release/gate-canary-e2e.json".into(),
            gate_canary_telemetry_path: "/tmp/release/gate-canary-telemetry.json".into(),
            telemetry_diag_path: "/tmp/release/gate-canary-telemetry-diagnostics.json".into(),
            canary_state: "e2e-passed".into(),
            eligibility: "ready".into(),
            phase: Some("e2e".into()),
            detail: None,
            state_status: Some("success".into()),
            has_remote_gate: true,
            has_telemetry_gate: true,
            has_e2e_gate: true,
            has_telemetry_diag: true,
            release_identity_ok: true,
            canary_public_url: Some("https://example.invalid".into()),
        };
        assert!(should_trigger_production_promotion_with_gate(&view, false));
    }

    #[test]
    fn auto_promotion_stops_when_prod_gate_already_exists() {
        let version = "ci-abcdef123456";
        let attempt = ReleaseAttempt {
            id: 1,
            project_id: 2,
            ref_name: "main".into(),
            sha: "abcdef1234567890".into(),
            version: version.into(),
            upstream_pipeline_id: Some(77),
            upstream_status: "success".into(),
            release_pipeline_id: Some(88),
            release_pipeline_status: Some("success".into()),
            production_pipeline_id: None,
            production_pipeline_status: None,
            canary_status: "passed".into(),
            canary_started_at: Some("2026-04-16T00:00:00Z".into()),
            canary_finished_at: Some("2026-04-16T00:10:00Z".into()),
            canary_note: Some("done".into()),
            created_at: "2026-04-16T00:00:00Z".into(),
            updated_at: "2026-04-16T00:10:01Z".into(),
        };
        let view = ReleaseAttemptView {
            attempt,
            release_dir: format!("/tmp/{version}"),
            canary_state_path: format!("/tmp/{version}/deploy-canary-c-state.json"),
            gate_remote_canary_path: format!("/tmp/{version}/gate-remote-canary.json"),
            gate_canary_e2e_path: format!("/tmp/{version}/gate-canary-e2e.json"),
            gate_canary_telemetry_path: format!("/tmp/{version}/gate-canary-telemetry.json"),
            telemetry_diag_path: format!("/tmp/{version}/gate-canary-telemetry-diagnostics.json"),
            canary_state: "e2e-passed".into(),
            eligibility: "ready".into(),
            phase: Some("e2e".into()),
            detail: None,
            state_status: Some("success".into()),
            has_remote_gate: true,
            has_telemetry_gate: true,
            has_e2e_gate: true,
            has_telemetry_diag: true,
            release_identity_ok: true,
            canary_public_url: Some("https://example.invalid".into()),
        };
        assert!(!should_trigger_production_promotion_with_gate(&view, true));
    }

    #[test]
    fn canary_gate_files_capture_complete_and_promotion_readiness() {
        let complete = CanaryGateFiles {
            remote: true,
            telemetry: true,
            e2e: true,
            validation: true,
            handoff: true,
            telemetry_diag: false,
        };
        assert!(complete.canary_complete());
        assert!(complete.promotion_ready());

        let incomplete = CanaryGateFiles {
            validation: false,
            ..complete
        };
        assert!(!incomplete.canary_complete());
        assert!(!incomplete.promotion_ready());
    }

    #[test]
    fn omitted_jobs_do_not_count_as_pending_for_successful_pipeline() {
        let schema = vec![
            CiSchemaJob {
                id: "compile-workspace".into(),
                lane: "release-blocking".into(),
                release_blocking: true,
                section: "Clean Checkout".into(),
                summary: "workspace check".into(),
                runner_tags: "build".into(),
                runner_pool: "build".into(),
                kind: "compile".into(),
                component: "workspace-build".into(),
                pipeline_product: "main-candidate".into(),
                evidence_driven: false,
                depends_on: vec![],
                evidence_outputs: vec![],
                estimated_cost: "medium".into(),
            },
            CiSchemaJob {
                id: "test-rust-nextest-4".into(),
                lane: "release-blocking".into(),
                release_blocking: true,
                section: "Bootstrap Stability".into(),
                summary: "workspace nextest partition".into(),
                runner_tags: "build".into(),
                runner_pool: "build".into(),
                kind: "contract".into(),
                component: "workspace-nextest".into(),
                pipeline_product: "main-candidate".into(),
                evidence_driven: false,
                depends_on: vec!["compile-workspace".into()],
                evidence_outputs: vec![],
                estimated_cost: "heavy".into(),
            },
        ];
        let mut statuses = HashMap::new();
        statuses.insert(
            "compile-workspace".to_string(),
            AggregatedPipelineJob {
                status: "success".into(),
                stage: Some("compile".into()),
            },
        );

        let lane = pipeline_lane_progress(&schema, &statuses, "release-blocking", "success");
        assert_eq!(lane.passed, 1);
        assert_eq!(lane.total, 1);
        assert_eq!(effective_job_status(None, "success"), "omitted");
        assert_eq!(effective_job_status(None, "running"), "pending");
    }

    #[test]
    fn selected_vti_graph_omits_absent_schema_jobs() {
        let schema = vec![
            CiSchemaJob {
                id: "compile-workspace".into(),
                lane: "release-blocking".into(),
                release_blocking: true,
                section: "Clean Checkout".into(),
                summary: "workspace check".into(),
                runner_tags: "build".into(),
                runner_pool: "build".into(),
                kind: "compile".into(),
                component: "workspace-build".into(),
                pipeline_product: "main-candidate".into(),
                evidence_driven: false,
                depends_on: vec![],
                evidence_outputs: vec![],
                estimated_cost: "medium".into(),
            },
            CiSchemaJob {
                id: "lint-shell".into(),
                lane: "release-blocking".into(),
                release_blocking: true,
                section: "Static Analysis".into(),
                summary: "shell lint".into(),
                runner_tags: "default".into(),
                runner_pool: "default".into(),
                kind: "lint".into(),
                component: "shell".into(),
                pipeline_product: "main-candidate".into(),
                evidence_driven: false,
                depends_on: vec![],
                evidence_outputs: vec![],
                estimated_cost: "small".into(),
            },
        ];
        let mut aggregated = HashMap::new();
        aggregated.insert(
            "compile-workspace".to_string(),
            AggregatedPipelineJob {
                status: "success".into(),
                stage: Some("compile".into()),
            },
        );
        let metadata = VtiGraphMetadata {
            selected_graph: true,
            materialized_jobs: HashSet::from(["compile-workspace".to_string()]),
        };

        apply_vti_selected_omissions(&schema, &metadata, &mut aggregated);

        assert_eq!(
            aggregated.get("lint-shell").map(|job| job.status.as_str()),
            Some("vti-skipped")
        );
        let lane = pipeline_lane_progress(&schema, &aggregated, "release-blocking", "running");
        assert_eq!(lane.passed, 1);
        assert_eq!(lane.total, 1);
    }

    #[test]
    fn allow_failure_release_candidate_jobs_do_not_block_reconcile() {
        let jobs = vec![
            job("build-enclave-server", "success", true),
            job("test-local-built", "failed", true),
            job("test-local-rc", "success", true),
        ];

        assert!(jobs_materialize_release_candidate(&jobs));
        assert!(failed_release_candidate_jobs(&jobs).is_empty());
    }

    #[test]
    fn hard_release_candidate_failures_still_block_reconcile() {
        let jobs = vec![
            job("build-enclave-server", "success", true),
            job("test-local-rc", "failed", false),
        ];

        assert_eq!(
            failed_release_candidate_jobs(&jobs),
            vec!["test-local-rc".to_string()]
        );
    }
}
