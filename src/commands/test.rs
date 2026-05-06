use crate::cli::TestCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{state, test_intel, test_runner};
use std::path::PathBuf;

/// Parse a comma-separated string into a trimmed, non-empty `Vec<String>`.
fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Convert an optional comma-separated `--tags` argument into the
/// `Option<Vec<String>>` shape that the test runner expects. Returns
/// `None` when the user did not pass `--tags` (so the runner can fall
/// back to its default tag inference).
fn parse_tag_list(tags: Option<String>) -> Option<Vec<String>> {
    tags.map(|raw| split_csv(&raw))
}

/// Resolve the current commit SHA via `git log -1 --format=%H`,
/// falling back to the literal string `"latest"` when git is
/// unavailable or the working copy has no commits.
fn current_commit_sha() -> String {
    match std::process::Command::new("git")
        .args(["log", "-1", "--format=%H"])
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Err(_) => "latest".to_string(),
    }
}

/// Run `git diff --name-only <base> <head>` from `cwd` and return the
/// list of changed paths. Used by every test command that needs the
/// diff between two refs to drive selection.
fn git_diff_changed_paths(cwd: &std::path::Path, base: &str, head: &str) -> Result<Vec<String>> {
    let diff_output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(["diff", "--name-only", base, head])
        .output()?;
    if !diff_output.status.success() {
        anyhow::bail!(
            "git diff failed: {}",
            String::from_utf8_lossy(&diff_output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&diff_output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

/// Serialize `value` as pretty JSON to `path`, creating parent
/// directories as needed, and emit a stderr breadcrumb describing what
/// was written.
fn write_json_artifact<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
    description: &str,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    eprintln!("Wrote {} to {}", description, path.display());
    Ok(())
}

/// Resolve audit/learn inputs (CSV strings + optional workspace) into a
/// computed [`test_intel::nightly::AuditReport`]. Used by both
/// `TestCommands::Audit` and `TestCommands::Learn` so the parsing,
/// optional testmap load, and `audit_selector` invocation only live in
/// one place.
fn build_audit_report(
    changed: &str,
    failed: &str,
    all_tests: &str,
    sha: &str,
    workspace: Option<&PathBuf>,
) -> test_intel::nightly::AuditReport {
    let changed_paths = split_csv(changed);
    let failed_tests = split_csv(failed);
    let all_test_list = split_csv(all_tests);

    let mut external_map = None;
    if let Some(workspace_path) = workspace {
        let testmap_path = workspace_path.join(".jeryu/testmap.toml");
        if testmap_path.exists() {
            external_map = match test_intel::testmap::load_testmap(&testmap_path) {
                Ok(m) => Some(m),
                Err(e) => {
                    eprintln!("Warning: failed to load testmap: {}", e);
                    None
                }
            };
        }
    }

    test_intel::nightly::audit_selector(
        &changed_paths,
        &failed_tests,
        &all_test_list,
        sha,
        external_map.as_ref(),
    )
}

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
        TestCommands::Retry {
            pipeline_id,
            job_name,
            project_id,
        } => {
            println!(
                "🔄 Retrying job '{}' in pipeline {}...",
                job_name, pipeline_id
            );
            let result =
                test_runner::retry_job_by_name(&client, project_id, pipeline_id, &job_name).await?;

            if result.passed {
                println!("✅ Job '{}' passed on retry!", job_name);
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
            let output = tokio::process::Command::new("cargo")
                .current_dir(&repo_root)
                .args([
                    "run",
                    "-q",
                    "-p",
                    "veox-testctl",
                    "--",
                    "ci-impact",
                    "--base",
                    &base,
                    "--head",
                    &head,
                    "--json",
                ])
                .output()
                .await?;
            if !output.status.success() {
                anyhow::bail!(
                    "ci-impact failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            if json {
                print!("{}", String::from_utf8_lossy(&output.stdout));
            } else {
                let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
                println!("━━━ jeryu test impact ━━━\n");
                println!("  Base/head:          {base}..{head}");
                let release_impacting = match value["release_impacting"].as_bool() {
                    Some(v) => v,
                    None => true,
                };
                let full_build_required = match value["full_build_required"].as_bool() {
                    Some(v) => v,
                    None => true,
                };
                println!("  Release impacting:  {}", release_impacting);
                println!("  Full build:         {}", full_build_required);
                let jobs = match value["jobs"].as_array() {
                    Some(items) => items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    None => String::new(),
                };
                println!("  Jobs:               {jobs}");
                let rules = match value["matched_rules"].as_array() {
                    Some(items) => items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    None => String::new(),
                };
                println!("  Matched rules:      {rules}");
            }
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

            let changed_paths = git_diff_changed_paths(&cwd, &base, &head)?;

            // Run the VTI planner
            let plan = test_intel::planner::plan_tests(&changed_paths);
            let receipt = plan.receipt(Some(&base), Some(&head));

            // Output
            if json {
                let json_value = test_intel::explain::explain_json(&plan);
                println!("{}", serde_json::to_string_pretty(&json_value)?);
            } else if explain {
                print!("{}", test_intel::explain::explain(&plan));
            } else {
                println!("━━━ jeryu smart test pick ━━━\n");
                println!("  Base:       {}", base);
                println!("  Head:       {}", head);
                println!("  Changed:    {} files", changed_paths.len());
                println!("  Mode:       {:?}", plan.mode);
                println!("  Confidence: {:.2}", plan.confidence);
                println!("  Receipt:    {}", receipt.receipt_id);
                println!("  Selected:   {} test commands", plan.selected_tests.len());
                println!("  Skipped:    {} subsystems", plan.skipped_subsystems.len());
                if let Some(reason) = plan.recovery_reason() {
                    println!("  Recovery:   {}", reason);
                }
                println!();
                for test in &plan.selected_tests {
                    println!("  ✓ [{}] {}", test.subsystem, test.command);
                }
            }

            // Emit artifacts
            if let Some(gitlab_path) = emit_gitlab {
                let yaml = test_intel::ci_gen::emit_gitlab_child_yaml(&plan);
                if let Some(parent) = gitlab_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&gitlab_path, &yaml)?;
                eprintln!("Wrote GitLab child pipeline to {}", gitlab_path.display());
            }

            if let Some(plan_path) = emit_plan {
                let json_value = test_intel::explain::explain_json(&plan);
                write_json_artifact(&plan_path, &json_value, "test plan")?;
            }

            if let Some(receipt_path) = emit_receipt {
                write_json_artifact(&receipt_path, &receipt, "VTI receipt")?;
            }
        }
        TestCommands::ExplainPlan { plan_path } => {
            let contents = std::fs::read_to_string(&plan_path)?;
            let plan: test_intel::planner::TestPlan = serde_json::from_str(&contents)?;
            print!("{}", test_intel::explain::explain(&plan));
        }
        TestCommands::SelectExternal {
            base,
            head,
            workspace,
            explain,
            json,
            emit_gitlab,
            emit_plan,
            emit_skipped,
        } => {
            let testmap_path = workspace.join(".jeryu/testmap.toml");
            if !testmap_path.exists() {
                anyhow::bail!("no .jeryu/testmap.toml found at {}", testmap_path.display());
            }

            let map =
                test_intel::testmap::load_testmap(&testmap_path).map_err(|e| anyhow::anyhow!(e))?;

            let changed_paths = git_diff_changed_paths(&workspace, &base, &head)?;

            let plan = test_intel::testmap::plan_from_testmap(&map, &changed_paths);

            // Output
            if json {
                let json_value = test_intel::testmap::explain_external_json(&plan);
                println!("{}", serde_json::to_string_pretty(&json_value)?);
            } else if explain {
                print!("{}", test_intel::testmap::explain_external_plan(&plan));
            } else {
                println!("━━━ jeryu test select-external ━━━\n");
                println!("  Workspace: {}", workspace.display());
                println!("  Base:      {}", base);
                println!("  Head:      {}", head);
                println!("  Changed:   {} files", changed_paths.len());
                println!("  Mode:      {:?}", plan.mode);
                println!("  Confidence:{:.2}", plan.confidence);
                println!("  Selected:  {} CI jobs", plan.selected_jobs.len());
                println!("  Skipped:   {} CI jobs", plan.skipped_jobs.len());
                if let Some(reason) = plan.recovery_reason() {
                    println!("  Recovery:  {}", reason);
                }
                println!();
                for job in &plan.selected_jobs {
                    println!("  ✓ {}", job);
                }
            }

            // Emit artifacts
            if let Some(gitlab_path) = emit_gitlab {
                let yaml = test_intel::testmap::emit_external_gitlab_yaml(&plan, Some(&workspace));
                if let Some(parent) = gitlab_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&gitlab_path, &yaml)?;
                eprintln!("Wrote GitLab child pipeline to {}", gitlab_path.display());
            }

            if let Some(plan_path) = emit_plan {
                let json_value = test_intel::testmap::explain_external_json(&plan);
                write_json_artifact(&plan_path, &json_value, "test plan")?;
            }

            if let Some(skipped_path) = emit_skipped {
                let json_value = test_intel::testmap::explain_external_skipped_json(&plan);
                write_json_artifact(&skipped_path, &json_value, "VTI skipped metadata")?;
            }
        }
        TestCommands::Audit {
            changed,
            failed,
            all_tests,
            sha,
            json,
            workspace,
        } => {
            let report =
                build_audit_report(&changed, &failed, &all_tests, &sha, workspace.as_ref());

            if json {
                let json_value = test_intel::nightly::explain_audit_json(&report);
                println!("{}", serde_json::to_string_pretty(&json_value)?);
            } else {
                print!("{}", test_intel::nightly::explain_audit(&report));
            }

            // Persist misses through the state store
            // (plan_id=None for CLI audits — no linked test_plans record)
            for miss in &report.misses {
                if let Err(e) = db
                    .record_selector_miss(
                        None,
                        &miss.missed_test,
                        &miss.failed_sha,
                        &miss.detected_by,
                    )
                    .await
                {
                    eprintln!("Warning: failed to persist selector miss: {}", e);
                }
            }
        }
        TestCommands::Learn {
            changed,
            failed,
            all_tests,
            sha,
            json,
            workspace,
        } => {
            let report =
                build_audit_report(&changed, &failed, &all_tests, &sha, workspace.as_ref());
            let result = test_intel::nightly::learn_from_audit(&report);

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "processed": result.processed,
                        "new_misses": result.new_misses,
                        "flagged_subsystems": result.flagged_subsystems,
                        "suggestions": result.suggestions,
                    }))?
                );
            } else {
                println!("━━━ VTI Learn ━━━\n");
                println!("  Processed: {} tests", result.processed);
                println!("  New misses: {}", result.new_misses);
                if !result.flagged_subsystems.is_empty() {
                    println!("  Flagged:   {}", result.flagged_subsystems.join(", "));
                }
                println!();
                for suggestion in &result.suggestions {
                    println!("  {}", suggestion);
                }
            }
        }
        TestCommands::CacheStatus { base, head, json } => {
            // 1. Get changed paths
            let diff_output = std::process::Command::new("git")
                .args(["diff", "--name-only", &base, &head])
                .output()?;
            let changed_paths: Vec<String> = String::from_utf8_lossy(&diff_output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();

            // 2. Compute test plan
            let plan = test_intel::planner::plan_tests(&changed_paths);

            // 3. Compute source hashes for changed files
            let mut source_hashes: Vec<(String, String)> = Vec::new();
            for path in &changed_paths {
                let hash_output = std::process::Command::new("git")
                    .args(["hash-object", path])
                    .output();
                let hash = match hash_output {
                    Ok(out) if out.status.success() => {
                        String::from_utf8_lossy(&out.stdout).trim().to_string()
                    }
                    _ => "unknown".to_string(),
                };
                source_hashes.push((path.clone(), hash));
            }

            // 4. Get Cargo.lock hash and rustc version
            let lock_hash = match std::process::Command::new("git")
                .args(["hash-object", "Cargo.lock"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                }
                _ => "no-lock".to_string(),
            };

            let rustc_ver = match std::process::Command::new("rustc")
                .args(["--version"])
                .output()
            {
                Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
                Err(_) => "unknown".to_string(),
            };

            // 5. Compute cache keys for each selected test
            let source_refs: Vec<(&str, &str)> = source_hashes
                .iter()
                .map(|(p, h)| (p.as_str(), h.as_str()))
                .collect();

            let tests_with_keys: Vec<(String, test_intel::cache::TestCacheKey)> = plan
                .selected_tests
                .iter()
                .map(|t| {
                    let key = test_intel::cache::compute_cache_key(
                        &t.command,
                        &source_refs,
                        &lock_hash,
                        &rustc_ver,
                        1, // epoch
                    );
                    (t.command.clone(), key)
                })
                .collect();

            // 6. Check against cache (empty for now — no persisted verdicts yet)
            let result = test_intel::cache::check_cache(&tests_with_keys, &[]);

            if json {
                let json_value = test_intel::cache::explain_cache_json(&result);
                println!("{}", serde_json::to_string_pretty(&json_value)?);
            } else {
                println!("━━━ jeryu test cache-status ━━━\n");
                println!("  Base: {}", base);
                println!("  Head: {}", head);
                println!("  Changed: {} files", changed_paths.len());
                println!(
                    "  Plan: {:?}, {} commands",
                    plan.mode,
                    plan.selected_tests.len()
                );
                println!();
                print!("{}", test_intel::cache::explain_cache_lookup(&result));
            }
        }
    }
    Ok(())
}
