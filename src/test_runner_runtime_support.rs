use anyhow::{Context, Result};
use std::time::Duration;
use tokio::time::sleep;

use crate::gitlab_client::GitlabClient;
use crate::test_runner::TestRunResult;

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
    let mut retried_source_fetch_auth = false;

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

        if let Some(job) = jobs
            .iter()
            .filter(|j| j.name == job_name)
            .max_by_key(|j| j.id)
        {
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
                    if crate::ci_failure::is_source_fetch_auth_failure(&trace) {
                        if !retried_source_fetch_auth {
                            tracing::warn!(
                                project_id,
                                pipeline_id,
                                job_id = job.id,
                                job_name,
                                "source fetch auth failure detected; retrying once"
                            );
                            client
                                .requeue_job(project_id, job.id)
                                .await
                                .context("retry source-fetch auth failure")?;
                            retried_source_fetch_auth = true;
                            sleep(Duration::from_secs(3)).await;
                            continue;
                        }
                        return Ok(TestRunResult {
                            pipeline_id,
                            job_id: Some(job.id),
                            job_name: job_name.to_string(),
                            status: "infrastructure_failure".to_string(),
                            duration_secs: job.queued_duration,
                            trace_tail: format!(
                                "{}\n\n{}",
                                crate::ci_failure::source_fetch_auth_incident_summary(),
                                trace
                            ),
                            passed: false,
                        });
                    }
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

        if let Ok(pipeline) = client.get_pipeline(project_id, pipeline_id).await
            && matches!(
                pipeline.status.as_str(),
                "failed" | "canceled" | "skipped" | "success"
            )
        {
            let mut trace_tail = format!(
                "Pipeline {pipeline_id} reached terminal status '{}' before job '{}' appeared.",
                pipeline.status, job_name
            );
            if let Some(web_url) = pipeline.web_url.as_deref() {
                trace_tail.push_str(&format!("\nweb_url: {web_url}"));
            }
            if let Some(yaml_errors) = pipeline.yaml_errors.as_deref() {
                trace_tail.push_str(&format!("\nyaml_errors: {yaml_errors}"));
            }
            trace_tail.push_str("\nNo job trace was available.");

            return Ok(TestRunResult {
                pipeline_id,
                job_id: None,
                job_name: job_name.to_string(),
                status: format!("pipeline_{}", pipeline.status),
                duration_secs: None,
                trace_tail,
                passed: false,
            });
        }

        sleep(Duration::from_secs(3)).await;
    }
}
