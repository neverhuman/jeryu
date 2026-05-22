use super::*;
use anyhow::Result;
use tracing::{info, warn};

#[path = "lifecycle_launch_trigger.rs"]
mod trigger;

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

    trigger::perform_canary_launch(db, client, project_id, ref_name, sha, version, pipeline_id)
        .await
}
