use super::*;
use crate::capability_records::CapabilityRepo;

pub(crate) async fn get_system_snapshot(
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(repo) = CapabilityRepo::open_default().await else {
        return err("state store unavailable");
    };
    let snapshot = match repo.system_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(e) => return err(&format!("system snapshot: {}", e)),
    };
    let recent_jobs = snapshot
        .recent_jobs
        .into_iter()
        .map(|event| {
            serde_json::json!({
                "job_id": event.job_id,
                "project_id": event.project_id,
                "pipeline_id": event.pipeline_id,
                "status": event.status,
                "job_name": event.job_name,
                "pool_name": event.pool_name,
                "system_id": event.system_id,
                "queued_duration": event.queued_duration,
                "received_at": event.received_at,
            })
        })
        .collect::<Vec<_>>();
    let latest_release = snapshot.latest_release.map(|attempt| {
        serde_json::json!({
            "id": attempt.id,
            "project_id": attempt.project_id,
            "ref_name": attempt.ref_name,
            "sha": attempt.sha,
            "version": attempt.version,
            "upstream_pipeline_id": attempt.upstream_pipeline_id,
            "upstream_status": attempt.upstream_status,
            "release_pipeline_id": attempt.release_pipeline_id,
            "release_pipeline_status": attempt.release_pipeline_status,
            "production_pipeline_id": attempt.production_pipeline_id,
            "production_pipeline_status": attempt.production_pipeline_status,
            "canary_status": attempt.canary_status,
            "canary_started_at": attempt.canary_started_at,
            "canary_finished_at": attempt.canary_finished_at,
            "canary_note": attempt.canary_note,
            "created_at": attempt.created_at,
            "updated_at": attempt.updated_at,
        })
    });
    let gitlab_ready = client.is_ready().await;
    CapabilityResponse {
        success: true,
        message: "system snapshot".into(),
        data: Some(serde_json::json!({
            "gitlab_ready": gitlab_ready,
            "pool_count": snapshot.pools.len(),
            "recent_job_events": recent_jobs,
            "latest_release": latest_release,
        })),
    }
}

pub(crate) async fn get_pipeline_jobs(
    project_id: i64,
    pipeline_id: i64,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    match client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await
    {
        Ok(jobs) => CapabilityResponse {
            success: true,
            message: "pipeline jobs".into(),
            data: Some(serde_json::json!({
                "project_id": project_id,
                "pipeline_id": pipeline_id,
                "jobs": jobs.into_iter().map(|job| {
                    serde_json::json!({
                        "id": job.id,
                        "name": job.name,
                        "status": job.status,
                        "stage": job.stage,
                        "allow_failure": job.allow_failure,
                        "ref_name": job.ref_name,
                        "web_url": job.web_url,
                        "queued_duration": job.queued_duration,
                        "duration": job.duration,
                        "started_at": job.started_at,
                        "finished_at": job.finished_at,
                        "runner": job.runner.and_then(|runner| runner.description),
                    })
                }).collect::<Vec<_>>()
            })),
        },
        Err(e) => err(&format!("pipeline jobs: {}", e)),
    }
}
