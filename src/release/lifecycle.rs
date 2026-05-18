use super::*;

#[path = "lifecycle_checks.rs"]
mod checks;
#[path = "lifecycle_launch.rs"]
mod launch;
#[path = "lifecycle_support.rs"]
mod support;

pub(crate) use checks::{ReleaseLock, release_lock_path, write_release_lock};
pub use checks::{release_doctor, release_preflight};
pub use launch::launch_canary_for_green_pipeline;
pub(crate) use support::{
    pipeline_has_release_execution_jobs, release_impacting_change, upstream_image_handoff,
};

pub async fn reconcile_release_for_ref(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    fresh: bool,
) -> Result<ReleaseStatusReport> {
    let recent_attempt = db
        .recent_release_attempts(Some(project_id), Some(ref_name), 20)
        .await?
        .into_iter()
        .next();

    if !fresh
        && let Some(existing) = recent_attempt
            .as_ref()
            .filter(|attempt| should_resume_existing_release_attempt_for_reconcile(attempt))
    {
        info!(
            project_id,
            ref_name = %ref_name,
            sha = %existing.sha,
            version = %existing.version,
            upstream_pipeline_id = ?existing.upstream_pipeline_id,
            release_pipeline_id = ?existing.release_pipeline_id,
            production_pipeline_id = ?existing.production_pipeline_id,
            selection_mode = "resume-existing",
            "resuming existing release attempt instead of selecting a new pipeline"
        );
        return reconcile_existing_release_attempt(
            db,
            client,
            project_id,
            ref_name,
            existing.clone(),
        )
        .await;
    }

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

    if fresh {
        if let Some(existing) = recent_attempt.as_ref() {
            if should_resume_existing_release_attempt_for_reconcile(existing) {
                info!(
                    project_id,
                    ref_name = %ref_name,
                    sha = %existing.sha,
                    version = %existing.version,
                    upstream_pipeline_id = ?existing.upstream_pipeline_id,
                    release_pipeline_id = ?existing.release_pipeline_id,
                    production_pipeline_id = ?existing.production_pipeline_id,
                    selection_mode = "fresh-selection",
                    "fresh reconcile requested; selecting a new upstream pipeline instead of resuming the active release attempt"
                );
            } else {
                info!(
                    project_id,
                    ref_name = %ref_name,
                    selection_mode = "fresh-selection",
                    "fresh reconcile requested; selecting a new upstream pipeline"
                );
            }
        }
    } else {
        info!(
            project_id,
            ref_name = %ref_name,
            upstream_pipeline_id = pipeline.id,
            sha = %pipeline.sha,
            status = %pipeline.status,
            selection_mode = "new-selection",
            "no resumable release attempt found; selecting the latest successful upstream pipeline"
        );
    }

    reconcile_new_release_attempt(db, client, project_id, ref_name, pipeline).await
}

pub(crate) fn should_resume_existing_release_attempt_for_reconcile(
    attempt: &ReleaseAttempt,
) -> bool {
    attempt.release_pipeline_id.is_some()
        && attempt.canary_status != "skipped"
        && attempt.production_pipeline_status.as_deref() != Some("success")
}

async fn reconcile_existing_release_attempt(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    attempt: ReleaseAttempt,
) -> Result<ReleaseStatusReport> {
    let version = attempt.version.clone();
    let sha = attempt.sha.clone();
    let mut existing = Some(attempt);
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
            db.finish_release_canary(project_id, ref_name, &sha, "failed", Some(&note))
                .await?;
            existing = db.get_release_attempt(project_id, ref_name, &sha).await?;
        }
    }
    let _existing_canary_status = existing
        .as_ref()
        .map(|attempt| attempt.canary_status.as_str())
        .unwrap_or("pending");
    if _existing_canary_status == "passed"
        && !has_complete_canary_evidence(&release_evidence(&version, &sha)?)
    {
        let note = "release-execution pipeline ended without required canary gate evidence";
        db.finish_release_canary(project_id, ref_name, &sha, "failed", Some(note))
            .await?;
    }

    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: Some(sha.clone()),
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

async fn reconcile_new_release_attempt(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    pipeline: Pipeline,
) -> Result<ReleaseStatusReport> {
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
