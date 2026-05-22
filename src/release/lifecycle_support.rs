use super::*;
use anyhow::{Context, Result};
use std::collections::HashMap;
use tokio::process::Command;
use tracing::warn;

pub(crate) async fn pipeline_has_release_execution_jobs(
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
pub(crate) struct UpstreamImageHandoff {
    pub(crate) artifact_pipeline_id: i64,
    pub(crate) build_job_id: i64,
    pub(crate) image_ref: String,
}

pub(crate) fn parse_image_env(raw: &str) -> HashMap<String, String> {
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

pub(crate) async fn upstream_image_handoff(
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

pub(crate) async fn release_impacting_change(sha: &str) -> Result<bool> {
    let root = crate::settings::release_repo_root();
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
