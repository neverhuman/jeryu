//! Owner: CI Test Runner runtime
//! Proof: `cargo nextest run -p jeryu -- test_runner`
//! Invariants: Test execution preserves lane semantics and reports enough structure for VTI feedback.

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

use crate::gitlab_client::GitlabClient;
use crate::test_runner::{TestRunOpts, TestRunResult, render_ephemeral_ci_yaml};

/// Run a single test command via a dynamic CI pipeline.
///
/// Creates a scratch branch, commits a minimal `.gitlab-ci.yml` with the
/// test command, triggers a pipeline, waits for it to finish, then cleans up.
pub async fn run_test(
    db: &crate::state::Db,
    client: &GitlabClient,
    opts: &TestRunOpts,
) -> Result<TestRunResult> {
    let start_time = tokio::time::Instant::now();
    let plan = crate::test_runner::plan_test_run(opts);

    if !opts.force
        && opts.commit_sha != "latest"
        && !opts.commit_sha.is_empty()
        && let Ok(Some(cached_run)) = db
            .latest_successful_test_execution(&opts.test_command)
            .await
    {
        let cached_sha = &cached_run.version;
        let mut can_skip = false;
        let mut skip_reason = String::new();

        if cached_sha == &opts.commit_sha {
            can_skip = true;
            skip_reason = "Exact commit cache hit".to_string();
        } else if cached_sha != "latest" && !cached_sha.is_empty() {
            // Determine impact between the cached and current revisions.
            if let Ok(impact_plan) =
                crate::impact::plan_for_push(client, opts.project_id, cached_sha, &opts.commit_sha)
                    .await
            {
                if impact_plan.selected_lanes.len() == 1
                    && impact_plan
                        .selected_lanes
                        .contains(&crate::decision::ImpactLane::DocsOnly)
                {
                    can_skip = true;
                    skip_reason = "Impact Analysis: DocsOnly cache hit".to_string();
                } else if !impact_plan
                    .selected_lanes
                    .contains(&crate::decision::ImpactLane::Full)
                {
                    // Advanced heuristics if needed, for instance if unit tests were requested but impact was only integration.
                    // We will rely on DocsOnly caching as the safest boundary for now before Canary testing.
                }
            }
        }

        if can_skip {
            tracing::info!(test_command = %opts.test_command, reason = %skip_reason, "test skipped: internal database validated cached test is still valid");
            return Ok(TestRunResult {
                pipeline_id: 0,
                job_id: None,
                job_name: plan.job_name,
                status: "success".to_string(),
                duration_secs: Some(0.0),
                trace_tail: format!(
                    "Test skipped.\n✅ Auto-pruned by jeryu.\nReason: The internal database determined the cached test is still valid ({skip_reason}).\nNote: Supply --force to override this optimization."
                ),
                passed: true,
            });
        }
    }

    let branch_name = format!(
        "jeryu-test-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
    );

    let ci_yaml = render_ephemeral_ci_yaml(&plan);

    info!(
        project_id = opts.project_id,
        branch = %branch_name,
        command = %plan.command,
        tags = ?plan.tags,
        risk_class = %plan.risk_class,
        "creating ephemeral test pipeline"
    );

    // 1. Create branch from main
    client
        .create_branch(opts.project_id, &branch_name, "main")
        .await
        .context("failed to create test branch")?;

    // 2. Commit the dynamic CI yaml
    // Use create_or_replace to handle both cases
    if client
        .update_file(
            opts.project_id,
            &branch_name,
            ".gitlab-ci.yml",
            &ci_yaml,
            &format!("[jeryu] test run: {}", plan.command),
        )
        .await
        .is_err()
    {
        // If write failed (file might not exist on branch yet), try creating via the
        // commits API with "create" action
        create_file_on_branch(
            client,
            opts.project_id,
            &branch_name,
            ".gitlab-ci.yml",
            &ci_yaml,
            &format!("[jeryu] test run: {}", plan.command),
        )
        .await
        .context("failed to commit test CI yaml")?;
    }

    // 3. The commit triggers a pipeline automatically. Find it.
    //    Note: creating the branch also triggers a pipeline with the FULL CI
    //    config from main. We need to find the LATEST pipeline (from our commit)
    //    and cancel any older ones to avoid consumer runner slots.
    //    GitLab may take several seconds to register the pipeline under load,
    //    so we poll again with escalating delays.
    let mut pipelines = Vec::new();
    for attempt in 0..5u32 {
        let delay = Duration::from_secs(3 + (attempt as u64) * 2);
        sleep(delay).await;
        pipelines = client
            .list_pipelines(opts.project_id, Some(&branch_name))
            .await
            .context("failed to list pipelines for test branch")?;
        if !pipelines.is_empty() {
            break;
        }
    }

    let pipeline_id = if let Some(pipeline) = pipelines.first() {
        pipeline.id
    } else {
        info!(
            branch = %branch_name,
            "branch pipeline not visible yet; triggering one explicitly"
        );
        client
            .trigger_pipeline(opts.project_id, &branch_name, Vec::new())
            .await
            .context("failed to trigger recovery test pipeline")?
    };

    // Cancel any older pipelines on this branch (from the branch-create event)
    for p in &pipelines[1..] {
        if matches!(p.status.as_str(), "pending" | "running" | "created") {
            info!(
                pipeline_id = p.id,
                "canceling spurious branch-create pipeline"
            );
            let _ = client.cancel_pipeline(opts.project_id, p.id).await;
        }
    }

    info!(
        pipeline_id,
        "ephemeral test pipeline started, waiting for completion"
    );

    // 4. Wait for pipeline to complete
    let result = wait_for_test_result(
        client,
        opts.project_id,
        pipeline_id,
        &plan.job_name,
        plan.timeout_secs,
    )
    .await?;

    // 5. Clean up: remove the scratch branch
    if let Err(e) = client.delete_branch(opts.project_id, &branch_name).await {
        tracing::warn!(error = %e, branch = %branch_name, "failed to clean up test branch");
    }

    let duration_ms = start_time.elapsed().as_millis() as i64;
    let version_to_record = if opts.commit_sha.is_empty() {
        "latest"
    } else {
        &opts.commit_sha
    };
    let _ = db
        .record_test_execution(
            &opts.test_command,
            version_to_record,
            duration_ms,
            &result.status,
        )
        .await;

    Ok(result)
}

/// Create a file on a branch using the "create" action.
async fn create_file_on_branch(
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

/// Wait for a pipeline to appear on a branch.
#[allow(dead_code)]
async fn wait_for_pipeline(
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

/// Wait for a test job within a pipeline to reach a terminal state.
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

        // Find our job (may have a suffix if requeued)
        if let Some(job) = jobs.iter().find(|j| j.name == job_name) {
            match job.status.as_str() {
                "success" => {
                    let trace = match client
                        .get_job_log_snippet(project_id, job.id, 2000)
                        .await
                    {
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
                    let trace = match client
                        .get_job_log_snippet(project_id, job.id, 4000)
                        .await
                    {
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
                _ => {
                    // Still running/pending/created
                }
            }
        }

        sleep(Duration::from_secs(3)).await;
    }
}

// Tests for this module live alongside test_runner.rs to avoid loading the
// same test file twice (clippy::duplicate_mod).
