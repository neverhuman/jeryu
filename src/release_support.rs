use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;
use tracing::{info, warn};

use crate::gitlab_client::GitlabClient;

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

pub const DEFAULT_RELEASE_PROJECT_ID: i64 = 48;

pub(crate) fn render_release_version(sha: &str) -> String {
    format!("ci-{}", sha.chars().take(12).collect::<String>())
}

pub(crate) fn release_dir(version: &str) -> PathBuf {
    crate::settings::release_repo_root()
        .join("ops/releases")
        .join(version)
}

pub(crate) fn canary_state_path(version: &str) -> PathBuf {
    release_dir(version).join("deploy-canary-c-state.json")
}

pub(crate) fn gate_remote_canary_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-remote-canary.json")
}

pub(crate) fn gate_canary_e2e_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-e2e.json")
}

pub(crate) fn gate_canary_telemetry_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-telemetry.json")
}

pub(crate) fn gate_prod_promotion_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-prod-promotion.json")
}

pub(crate) fn telemetry_diag_path(version: &str) -> PathBuf {
    release_dir(version).join("gate-canary-telemetry-diagnostics.json")
}

pub(crate) fn c_handoff_path(version: &str) -> PathBuf {
    release_dir(version).join("rendered/c-handoff.json")
}

pub(crate) fn c_validation_path(version: &str) -> PathBuf {
    release_dir(version).join("c-validation.json")
}

/// Download gate files and handoff artifacts from the deploy-canary-final job
/// of a release-execution pipeline to local disk. Non-fatal: logs and returns Ok
/// if the job is not found or individual artifacts are missing.
pub(crate) async fn sync_canary_artifacts(
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

pub(crate) fn canary_public_url(version: &str) -> Option<String> {
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
