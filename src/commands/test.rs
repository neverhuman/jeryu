use crate::cli::TestCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{state, test_intel, test_runner};
use std::path::PathBuf;

#[path = "test_back.rs"]
mod test_back;
use test_back::{current_commit_sha, handle_choose, handle_impact, parse_tag_list};

pub(crate) async fn execute_test_commands(subcmd: TestCommands) -> Result<()> {
    let (client, _) = load_client()?;
    let db = state::Db::open().await?;

    match subcmd {
        TestCommands::Run {
            command,
            project_id,
            image,
            tags,
            timeout,
            force,
        } => {
            let opts = test_runner::TestRunOpts {
                project_id,
                test_command: command,
                job_name: None,
                image,
                tags: parse_tag_list(tags),
                timeout_secs: timeout,
                force,
                commit_sha: current_commit_sha(),
            };
            println!("━━━ jeryu test run ━━━\n");
            println!("  Project ID: {}", opts.project_id);
            println!("  Command:    {}", opts.test_command);
            let plan = test_runner::plan_test_run(&opts);
            println!("  Inferred Routing:");
            println!("    Risk Class: {}", plan.risk_class);
            println!("    Tags:       {:?}", plan.tags);
            for reason in &plan.rationale {
                println!("      - {}", reason);
            }
            println!("\nExecuting pipeline...");

            let result = test_runner::run_test(&db, &client, &opts).await?;
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
        }
        TestCommands::Plan {
            command,
            project_id,
            image,
            tags,
            timeout,
        } => {
            let opts = test_runner::TestRunOpts {
                project_id,
                test_command: command,
                job_name: None,
                image,
                tags: parse_tag_list(tags),
                timeout_secs: timeout,
                force: false,
                commit_sha: String::new(),
            };
            println!("━━━ jeryu test plan ━━━\n");
            let plan = test_runner::plan_test_run(&opts);
            println!("  Command:      {}", plan.command);
            println!("  Risk Class:   {}", plan.risk_class);
            println!("  Tags:         {:?}", plan.tags);
            println!("  Timeout:      {}s", plan.timeout_secs);
            println!("  Rationale:");
            for reason in &plan.rationale {
                println!("    - {}", reason);
            }
        }
        TestCommands::Batch {
            commands,
            project_id,
            image,
            tags,
            timeout,
            max_parallel,
            force,
        } => {
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
            };
            println!("🧪 Starting batched test run...");
            println!("   Commands:  {}", opts.test_commands.len());
            println!("   Image:     {}", opts.image);
            let tags_label = match opts.tags.as_ref() {
                Some(tags) => format!("{:?}", tags),
                None => "smart-inferred".to_string(),
            };
            println!("   Tags:      {}", tags_label);
            println!("   Parallel:  {}", opts.max_parallel);
            println!();
            let results = test_runner::run_test_batch(&db, &client, &opts).await?;
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
        }
        TestCommands::Results {
            pipeline_id,
            project_id,
        } => {
            let results = test_runner::pipeline_results(&client, project_id, pipeline_id).await?;
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
        }
        TestCommands::Requeue {
            pipeline_id,
            job_name,
            project_id,
        } => {
            println!(
                "🔄 Requeuing job '{}' in pipeline {}...",
                job_name, pipeline_id
            );
            let result =
                test_runner::requeue_job_by_name(&client, project_id, pipeline_id, &job_name)
                    .await?;
            if result.passed {
                println!("✅ Job '{}' passed after requeue!", job_name);
            } else {
                println!("❌ Job '{}' still failing: {}", job_name, result.status);
            }
        }
        TestCommands::Failed {
            pipeline_id,
            project_id,
        } => {
            let results = test_runner::pipeline_results(&client, project_id, pipeline_id).await?;
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
        }
        TestCommands::Impact {
            base,
            head,
            repo_root,
            json,
        } => {
            handle_impact(base, head, repo_root, json).await?;
        }
        TestCommands::Choose {
            base,
            head,
            repo_root,
            explain,
            json,
            emit_gitlab,
            emit_plan,
            emit_receipt,
        } => {
            let cwd = match repo_root {
                Some(path) => path,
                None => match std::env::current_dir() {
                    Ok(dir) => dir,
                    Err(_) => PathBuf::from("."),
                },
            };
            handle_choose(
                base,
                head,
                cwd,
                explain,
                json,
                emit_gitlab,
                emit_plan,
                emit_receipt,
            )?;
        }
        TestCommands::ExplainPlan { plan_path } => {
            let contents = std::fs::read_to_string(&plan_path)?;
            let plan: test_intel::planner::TestPlan = serde_json::from_str(&contents)?;
            print!("{}", test_intel::explain::explain(&plan));
        }
        other => return test_back::run(other, &db).await,
    }
    Ok(())
}
