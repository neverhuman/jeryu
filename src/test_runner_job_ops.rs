//! Owner: CI Test Runner job operations
//! Proof: `cargo nextest run -p jeryu -- test_runner`
//! Invariants: Test execution preserves lane semantics and reports enough structure for VTI feedback.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::gitlab_client::GitlabClient;
use crate::test_runner::{
    TestBatchOpts, TestRunOpts, TestRunResult, run_test, wait_for_test_result,
};

/// Run several test commands in parallel through CI pipelines.
pub async fn run_test_batch(
    db: &crate::state::Db,
    client: &GitlabClient,
    opts: &TestBatchOpts,
) -> Result<Vec<TestRunResult>> {
    let max_parallel = opts.max_parallel.max(1);
    let semaphore = Arc::new(Semaphore::new(max_parallel));
    let mut join_set = JoinSet::new();

    for (index, command) in opts.test_commands.iter().cloned().enumerate() {
        let permit = semaphore.clone().acquire_owned().await?;
        let db = db.clone();
        let client = client.clone();
        let image = opts.image.clone();
        let tags = opts.tags.clone();
        let project_id = opts.project_id;
        let timeout_secs = opts.timeout_secs;
        let job_name_prefix = opts.job_name_prefix.clone();
        let force = opts.force;
        let commit_sha = opts.commit_sha.clone();
        join_set.spawn(async move {
            let _permit = permit;
            let job_name = job_name_prefix
                .map(|prefix| format!("{prefix}-{:02}", index + 1))
                .filter(|value| !value.is_empty());
            let run_opts = TestRunOpts {
                project_id,
                test_command: command.clone(),
                job_name,
                image,
                tags,
                timeout_secs,
                force,
                commit_sha,
            };
            let outcome = run_test(&db, &client, &run_opts).await;
            (index, command, outcome)
        });
    }

    let mut results: Vec<Option<TestRunResult>> = std::iter::repeat_with(|| None)
        .take(opts.test_commands.len())
        .collect();
    while let Some(joined) = join_set.join_next().await {
        let (index, command, outcome) = joined?;
        let result = match outcome {
            Ok(result) => result,
            Err(error) => TestRunResult {
                pipeline_id: 0,
                job_id: None,
                job_name: format!("batch-{index:02}"),
                status: "error".to_string(),
                duration_secs: None,
                trace_tail: format!("{command}\n\n{error:#}"),
                passed: false,
            },
        };
        results[index] = Some(result);
    }

    Ok(results.into_iter().flatten().collect())
}

/// Requeue a specific failed job from the latest pipeline.
pub async fn requeue_job_by_name(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    job_name: &str,
) -> Result<TestRunResult> {
    let jobs = client.list_pipeline_jobs(project_id, pipeline_id).await?;

    let Some(job) = jobs.iter().find(|j| j.name == job_name) else {
        return Err(anyhow::anyhow!(
            "job '{}' not found in pipeline {}",
            job_name,
            pipeline_id
        ));
    };

    if job.status == "failed" || job.status == "canceled" {
        client.requeue_job(project_id, job.id).await?;
        tracing::info!(project_id, job_id = job.id, job_name, "requeued job");

        // Wait for the requeued job to complete
        wait_for_test_result(client, project_id, pipeline_id, job_name, 600).await
    } else {
        Ok(TestRunResult {
            pipeline_id,
            job_id: Some(job.id),
            job_name: job_name.to_string(),
            status: job.status.clone(),
            duration_secs: job.queued_duration,
            trace_tail: String::new(),
            passed: job.status == "success",
        })
    }
}

/// Get the results of all jobs in a pipeline.
pub async fn pipeline_results(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<Vec<TestRunResult>> {
    let jobs = client.list_pipeline_jobs(project_id, pipeline_id).await?;

    let mut results = Vec::new();
    for job in &jobs {
        let trace_tail = if job.status == "failed" {
            match client
                .get_job_log_snippet(project_id, job.id, 2000)
                .await
            {
                Ok(s) => s,
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };

        results.push(TestRunResult {
            pipeline_id,
            job_id: Some(job.id),
            job_name: job.name.clone(),
            status: job.status.clone(),
            duration_secs: job.queued_duration,
            trace_tail,
            passed: job.status == "success",
        });
    }

    Ok(results)
}
