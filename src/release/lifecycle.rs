use super::*;
use tracing::info;

#[path = "lifecycle_checks.rs"]
mod checks;
#[path = "lifecycle_launch.rs"]
mod launch;
#[path = "lifecycle_reconcile.rs"]
mod reconcile;
#[path = "lifecycle_support.rs"]
mod support;

pub use checks::{DoctorReport, PreflightReport, release_doctor, release_preflight};
pub(crate) use checks::{ReleaseLock, release_lock_path, write_release_lock};
pub use launch::launch_canary_for_green_pipeline;
pub(crate) use reconcile::*;
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
