//! Owner: CI Test Runner runtime
//! Proof: `cargo nextest run -p jeryu -- test_runner`
//! Invariants: Test execution preserves lane semantics and reports enough structure for VTI feedback.

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

use crate::gitlab_client::GitlabClient;
use crate::test_runner::{TestRunOpts, TestRunResult, render_ephemeral_ci_yaml};

#[path = "test_runner_runtime_support.rs"]
mod support;
pub(crate) use support::wait_for_test_result;

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
    let lint = client
        .lint_ci_yaml(opts.project_id, &ci_yaml)
        .await
        .context("failed to lint generated test CI yaml")?;
    if !lint.valid {
        let errors = if lint.errors.is_empty() {
            "none".to_string()
        } else {
            lint.errors.join("; ")
        };
        let warnings = if lint.warnings.is_empty() {
            "none".to_string()
        } else {
            lint.warnings.join("; ")
        };
        anyhow::bail!(
            "generated test CI yaml failed GitLab lint: errors=[{errors}]; warnings=[{warnings}]"
        );
    }
    if !lint.warnings.is_empty() {
        tracing::warn!(
            project_id = opts.project_id,
            branch = %branch_name,
            warnings = ?lint.warnings,
            "generated test CI yaml lint returned warnings"
        );
    }

    info!(
        project_id = opts.project_id,
        branch = %branch_name,
        command = %plan.command,
        risk_class = %plan.risk_class,
        "creating ephemeral test pipeline"
    );

    // 1. Create branch from main
    client
        .create_branch(opts.project_id, &branch_name, "main")
        .await
        .context("failed to create test branch")?;

    // 2. Commit the dynamic CI yaml and keep the commit SHA so we can select
    //    the pipeline created by this exact commit rather than whichever branch
    //    pipeline GitLab surfaces first.
    let commit_message = format!("[jeryu] test run: {}", plan.command);
    let commit_sha = match client
        .commit_actions_with_sha(
            opts.project_id,
            &branch_name,
            &commit_message,
            &[("update", ".gitlab-ci.yml", &ci_yaml)],
        )
        .await
    {
        Ok(commit_sha) => commit_sha,
        Err(_) => client
            .commit_actions_with_sha(
                opts.project_id,
                &branch_name,
                &commit_message,
                &[("create", ".gitlab-ci.yml", &ci_yaml)],
            )
            .await
            .context("failed to commit test CI yaml")?,
    };
    info!(
        project_id = opts.project_id,
        branch = %branch_name,
        commit_sha = %commit_sha,
        "committed ephemeral test CI"
    );

    // 3. The commit triggers a pipeline automatically. Find the pipeline whose
    //    SHA matches this exact commit rather than relying on branch pipeline
    //    ordering from GitLab.
    let mut pipelines = Vec::new();
    let mut matching_pipeline_id = None;
    for attempt in 0..5u32 {
        let delay = Duration::from_secs(3 + (attempt as u64) * 2);
        sleep(delay).await;
        pipelines = client
            .list_pipelines(opts.project_id, Some(&branch_name))
            .await
            .context("failed to list pipelines for test branch")?;
        matching_pipeline_id = pipelines
            .iter()
            .filter(|pipeline| pipeline.sha == commit_sha)
            .max_by_key(|pipeline| pipeline.id)
            .map(|pipeline| pipeline.id);
        if matching_pipeline_id.is_some() {
            break;
        }
    }

    let pipeline_id = if let Some(pipeline_id) = matching_pipeline_id {
        pipeline_id
    } else {
        info!(
            branch = %branch_name,
            commit_sha = %commit_sha,
            "matching branch pipeline not visible yet; triggering one explicitly"
        );
        client
            .trigger_pipeline(opts.project_id, &branch_name, Vec::new())
            .await
            .context("failed to trigger recovery test pipeline")?
    };

    // Cancel any older or different-SHA non-terminal pipelines on this branch.
    for p in &pipelines {
        if p.id != pipeline_id
            && matches!(p.status.as_str(), "pending" | "running" | "created")
            && (p.sha != commit_sha || p.id < pipeline_id)
        {
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
