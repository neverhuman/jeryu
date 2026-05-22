use super::*;

pub async fn fetch_ci_job_runs(
    client: &gitlab_client::GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<Vec<state::CiJobRun>> {
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    let observed_at = chrono::Utc::now().to_rfc3339();
    Ok(jobs
        .into_iter()
        .map(|job| {
            let runner = job.runner.and_then(|runner| runner.description);
            let actual_pipeline_id = job.pipeline_id.unwrap_or(pipeline_id);
            state::CiJobRun {
                job_id: job.id,
                project_id,
                pipeline_id: actual_pipeline_id,
                root_pipeline_id: pipeline_id,
                pipeline_sha: pipeline.sha.clone(),
                ref_name: pipeline.ref_name.clone(),
                job_name: job.name,
                stage: job.stage,
                status: job.status,
                runner_pool: runner
                    .as_deref()
                    .and_then(infer_runner_pool)
                    .map(str::to_string),
                runner,
                queued_duration_secs: job.queued_duration,
                duration_secs: job.duration,
                started_at: job.started_at,
                finished_at: job.finished_at,
                web_url: job.web_url,
                observed_at: observed_at.clone(),
            }
        })
        .collect())
}

fn infer_runner_pool(runner: &str) -> Option<&'static str> {
    let lower = runner.to_ascii_lowercase();
    if lower.contains("untrusted") {
        Some("untrusted")
    } else if lower.contains("build") {
        Some("build")
    } else if lower.contains("default") {
        Some("default")
    } else {
        None
    }
}
