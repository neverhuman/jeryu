use super::*;
use anyhow::Result;
use tracing::warn;

#[path = "promotion_lookup.rs"]
mod lookup;

#[path = "promotion_trigger.rs"]
mod trigger;

pub use trigger::trigger_production_promotion;

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
            || !release_dir(&view.attempt.version)
                .join("release.json")
                .is_file()
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
        lookup::production_promotion_pipeline_id(client, project_id, ref_name, &view.attempt.sha)
            .await?
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
