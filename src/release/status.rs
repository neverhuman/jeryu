use super::*;

#[path = "status_pipeline.rs"]
mod pipeline_helpers;
pub(crate) use pipeline_helpers::*;

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
pub(crate) struct ReleaseEvidence {
    pub(crate) state_phase: Option<String>,
    pub(crate) state_status: Option<String>,
    pub(crate) state_detail: Option<String>,
    pub(crate) has_remote_gate: bool,
    pub(crate) has_telemetry_gate: bool,
    pub(crate) has_e2e_gate: bool,
    pub(crate) has_c_validation: bool,
    pub(crate) has_c_handoff: bool,
    pub(crate) has_telemetry_diag: bool,
    pub(crate) release_identity_ok: bool,
}

pub(crate) fn release_scope(query: &ReleaseStatusQuery) -> String {
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

pub(crate) async fn load_ci_schema(root: &Path) -> Result<CiSchema> {
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
pub(crate) struct AggregatedPipelineJob {
    pub(crate) status: String,
    pub(crate) stage: Option<String>,
}

pub(crate) fn lane_progress(
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
    let percent = lane_progress_percent(total, passed);
    LaneProgress {
        passed,
        total,
        percent,
    }
}

#[path = "release_status_render.rs"]
mod render;
pub use render::*;
#[path = "release_status_summary.rs"]
mod summary;
pub(crate) use summary::*;
