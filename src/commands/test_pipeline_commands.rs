use anyhow::Result;
use jeryu::{state, test_runner};

use jeryu::gitlab_client::GitlabClient;

use super::{current_commit_sha, parse_tag_list};

#[allow(clippy::too_many_arguments)] // CLI flag passthrough; this dispatcher is intentionally flat
pub(crate) async fn handle_run_command(
    client: &GitlabClient,
    db: &state::Db,
    command: String,
    project_id: i64,
    image: String,
    tags: Option<String>,
    timeout: u64,
    force: bool,
    priority: Option<test_runner::TestRunPriority>,
    reason: test_runner::TestRunReason,
) -> Result<()> {
    let opts = test_runner::TestRunOpts {
        project_id,
        test_command: command,
        job_name: None,
        image,
        tags: parse_tag_list(tags),
        timeout_secs: timeout,
        force,
        commit_sha: current_commit_sha(),
        priority,
        reason,
    };
    println!("━━━ jeryu test run ━━━\n");
    println!("  Project ID: {}", opts.project_id);
    println!("  Command:    {}", opts.test_command);
    let plan = test_runner::plan_test_run(&opts);
    println!("  Inferred Routing:");
    println!("    Risk Class: {}", plan.risk_class);
    println!("    Tags:       {:?}", plan.tags);
    println!(
        "    Scheduler:  {} ({})",
        plan.priority.label(),
        plan.reason.label()
    );
    for reason in &plan.rationale {
        println!("      - {}", reason);
    }
    println!("\nExecuting pipeline...");

    let result = test_runner::run_test(db, client, &opts).await?;
    println!(
        "\nResult: {}",
        if result.passed {
            "✅ Passed"
        } else {
            "❌ Failed"
        }
    );
    if let Some(dur) = result.duration_secs {
        println!("Duration: {:.1}s", dur);
    }
    if !result.trace_tail.is_empty() {
        println!("\nTrace tail:\n{}", result.trace_tail);
    }
    Ok(())
}

pub(crate) fn handle_plan_command(
    command: String,
    project_id: i64,
    image: String,
    tags: Option<String>,
    timeout: u64,
    priority: Option<test_runner::TestRunPriority>,
    reason: test_runner::TestRunReason,
) -> Result<()> {
    let opts = test_runner::TestRunOpts {
        project_id,
        test_command: command,
        job_name: None,
        image,
        tags: parse_tag_list(tags),
        timeout_secs: timeout,
        force: false,
        commit_sha: String::new(),
        priority,
        reason,
    };
    println!("━━━ jeryu test plan ━━━\n");
    let plan = test_runner::plan_test_run(&opts);
    println!("  Command:      {}", plan.command);
    println!("  Risk Class:   {}", plan.risk_class);
    println!(
        "  Scheduler:    {} ({})",
        plan.priority.label(),
        plan.reason.label()
    );
    println!("  Tags:         {:?}", plan.tags);
    println!("  Timeout:      {}s", plan.timeout_secs);
    println!("  Rationale:");
    for reason in &plan.rationale {
        println!("    - {}", reason);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // CLI flag passthrough; this dispatcher is intentionally flat
pub(crate) async fn handle_batch_command(
    client: &GitlabClient,
    db: &state::Db,
    commands: Vec<String>,
    project_id: i64,
    image: String,
    tags: Option<String>,
    timeout: u64,
    max_parallel: usize,
    force: bool,
    priority: Option<test_runner::TestRunPriority>,
    reason: test_runner::TestRunReason,
) -> Result<()> {
    let opts = test_runner::TestBatchOpts {
        project_id,
        test_commands: commands.clone(),
        job_name_prefix: Some("batch-test".to_string()),
        image,
        tags: parse_tag_list(tags),
        timeout_secs: timeout,
        max_parallel,
        force,
        commit_sha: current_commit_sha(),
        priority,
        reason,
    };
    println!("🧪 Starting batched test run...");
    println!("   Commands:  {}", opts.test_commands.len());
    println!("   Image:     {}", opts.image);
    let tags_label = match opts.tags.as_ref() {
        Some(tags) => format!("{:?}", tags),
        None => "smart-inferred".to_string(),
    };
    println!("   Tags:      {}", tags_label);
    println!(
        "   Scheduler: {} ({})",
        match opts.priority {
            Some(priority) => priority,
            None => opts.reason.default_priority(),
        }
        .label(),
        opts.reason.label()
    );
    println!("   Parallel:  {}", opts.max_parallel);
    println!();
    let results = test_runner::run_test_batch(db, client, &opts).await?;
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();
    println!("✅ Batch complete: {} passed, {} failed", passed, failed);
    for r in &results {
        let icon = if r.passed { "✅" } else { "❌" };
        println!(
            "  {} {:<34} {:<10} pipeline={}",
            icon, r.job_name, r.status, r.pipeline_id
        );
    }
    Ok(())
}

pub(crate) async fn handle_results_command(
    client: &GitlabClient,
    pipeline_id: i64,
    project_id: i64,
) -> Result<()> {
    let results = test_runner::pipeline_results(client, project_id, pipeline_id).await?;
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| r.status == "failed").count();
    let skipped = results.iter().filter(|r| r.status == "skipped").count();
    let other = results.len() - passed - failed - skipped;
    println!("Pipeline {} — {} jobs", pipeline_id, results.len());
    println!(
        "  ✅ {} passed  ❌ {} failed  ⏭ {} skipped  ⏳ {} other",
        passed, failed, skipped, other
    );
    println!();
    for r in &results {
        let icon = match r.status.as_str() {
            "success" => "✅",
            "failed" => "❌",
            "skipped" => "⏭ ",
            "running" => "🔄",
            "pending" | "created" => "⏳",
            _ => "❓",
        };
        let dur = match r.duration_secs {
            Some(d) => format!("{:.0}s", d),
            None => String::new(),
        };
        println!("  {} {:<40} {:>8} {}", icon, r.job_name, r.status, dur);
    }
    Ok(())
}

pub(crate) async fn handle_requeue_command(
    client: &GitlabClient,
    pipeline_id: i64,
    job_name: String,
    project_id: i64,
) -> Result<()> {
    println!(
        "🔄 Requeuing job '{}' in pipeline {}...",
        job_name, pipeline_id
    );
    let result =
        test_runner::requeue_job_by_name(client, project_id, pipeline_id, &job_name).await?;
    if result.passed {
        println!("✅ Job '{}' passed after requeue!", job_name);
    } else {
        println!("❌ Job '{}' still failing: {}", job_name, result.status);
    }
    Ok(())
}

pub(crate) async fn handle_failed_command(
    client: &GitlabClient,
    pipeline_id: i64,
    project_id: i64,
) -> Result<()> {
    let results = test_runner::pipeline_results(client, project_id, pipeline_id).await?;
    let failed: Vec<_> = results
        .into_iter()
        .filter(|r| r.status == "failed")
        .collect();
    if failed.is_empty() {
        println!("✅ No failed jobs in pipeline {}!", pipeline_id);
    } else {
        println!(
            "❌ {} failed job(s) in pipeline {}:\n",
            failed.len(),
            pipeline_id
        );
        for r in &failed {
            println!("━━━ {} (id={:?}) ━━━", r.job_name, r.job_id);
            if !r.trace_tail.is_empty() {
                let lines: Vec<&str> = r.trace_tail.lines().collect();
                let start = lines.len().saturating_sub(20);
                for line in &lines[start..] {
                    println!("  {}", line);
                }
            }
            println!();
        }
    }
    Ok(())
}
