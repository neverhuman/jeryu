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

pub const DEFAULT_RELEASE_PROJECT_ID: i64 = 2;

fn render_release_version(sha: &str) -> String {
    format!("ci-{}", sha.chars().take(12).collect::<String>())
}

fn release_dir(version: &str) -> PathBuf {
    crate::settings::release_repo_root()
        .join("ops/releases")
        .join(version)
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

#[path = "release_types.rs"]
mod types;
pub use types::*;

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

mod canary;
mod capsule;
// `foundry` is `pub mod` (not just `mod`) so the Wave 11.A db-boundary
// extraction in `src/db/release_repo.rs` can name its concrete types
// (`FoundryConfig`, `ReleaseCandidate`) via the explicit submodule path.
// Re-exports below (`pub use foundry::*`) still publish the same surface
// to outside crates.
pub mod foundry;
mod gate;
mod lifecycle;
mod pipeline;
mod progress;
mod rollback;
// Wave 3.5.B: SQL-backed FoundryQueue (sister of in-memory `foundry::FoundryTrain`).
// Re-exported so callers (notably the `autonomy foundry` CLI in `src/bin/autonomy.rs`)
// can swap from the in-memory `FoundryTrain` to the restart-durable
// `SqlFoundryQueue` without reaching into private module paths.
pub mod sql_foundry_queue;
mod status;
#[cfg(test)]
mod tests;

pub use canary::*;
pub use capsule::*;
pub use foundry::*;
pub use gate::*;
pub use lifecycle::*;
pub use pipeline::*;
pub use progress::*;
pub use rollback::*;
pub use sql_foundry_queue::SqlFoundryQueue;
pub use status::*;
