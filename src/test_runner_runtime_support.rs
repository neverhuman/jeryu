use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;

use crate::gitlab_client::GitlabClient;
use crate::test_runner::TestRunResult;

pub(crate) async fn create_file_on_branch(
    client: &GitlabClient,
    project_id: i64,
    branch: &str,
    file_path: &str,
    content: &str,
    message: &str,
) -> Result<()> {
    client
        .create_file(project_id, branch, file_path, content, message)
        .await
}

#[allow(dead_code)]
pub(crate) async fn wait_for_pipeline(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    max_attempts: u32,
) -> Result<i64> {
    for _ in 0..max_attempts {
        let pipelines = client.list_pipelines(project_id, Some(ref_name)).await?;
        if let Some(p) = pipelines.first() {
            return Ok(p.id);
        }
        sleep(Duration::from_secs(2)).await;
    }
    anyhow::bail!("no pipeline appeared for ref '{}' after waiting", ref_name)
}

pub(crate) async fn wait_for_test_result(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    job_name: &str,
    timeout_secs: u64,
) -> Result<TestRunResult> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        if tokio::time::Instant::now() > deadline {
            return Ok(TestRunResult {
                pipeline_id,
                job_id: None,
                job_name: job_name.to_string(),
                status: "timeout".to_string(),
                duration_secs: Some(timeout_secs as f64),
                trace_tail: "Timed out waiting for job to complete".to_string(),
                passed: false,
            });
        }

        let jobs = client.list_pipeline_jobs(project_id, pipeline_id).await?;

        if let Some(job) = jobs.iter().find(|j| j.name == job_name) {
            match job.status.as_str() {
                "success" => {
                    let trace = match client.get_job_log_snippet(project_id, job.id, 2000).await {
                        Ok(s) => s,
                        Err(_) => String::new(),
                    };
                    return Ok(TestRunResult {
                        pipeline_id,
                        job_id: Some(job.id),
                        job_name: job_name.to_string(),
                        status: "success".to_string(),
                        duration_secs: job.queued_duration,
                        trace_tail: trace,
                        passed: true,
                    });
                }
                "failed" => {
                    let trace = match client.get_job_log_snippet(project_id, job.id, 4000).await {
                        Ok(s) => s,
                        Err(_) => String::new(),
                    };
                    return Ok(TestRunResult {
                        pipeline_id,
                        job_id: Some(job.id),
                        job_name: job_name.to_string(),
                        status: "failed".to_string(),
                        duration_secs: job.queued_duration,
                        trace_tail: trace,
                        passed: false,
                    });
                }
                "canceled" | "skipped" => {
                    return Ok(TestRunResult {
                        pipeline_id,
                        job_id: Some(job.id),
                        job_name: job_name.to_string(),
                        status: job.status.clone(),
                        duration_secs: job.queued_duration,
                        trace_tail: String::new(),
                        passed: false,
                    });
                }
                _ => {}
            }
        }

        sleep(Duration::from_secs(3)).await;
    }
}
