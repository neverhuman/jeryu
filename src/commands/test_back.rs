use crate::cli::TestCommands;
use anyhow::Result;
use jeryu::{state, test_intel};
use std::path::PathBuf;

/// Parse a comma-separated string into a trimmed, non-empty `Vec<String>`.
pub(crate) fn split_csv(input: &str) -> Vec<String> {
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
pub(crate) fn parse_tag_list(tags: Option<String>) -> Option<Vec<String>> {
    tags.map(|raw| split_csv(&raw))
}

/// Resolve the current commit SHA via `git log -1 --format=%H`,
/// falling back to the literal string `"latest"` when git is
/// unavailable or the working copy has no commits.
pub(crate) fn current_commit_sha() -> String {
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
pub(crate) fn git_diff_changed_paths(
    cwd: &std::path::Path,
    base: &str,
    head: &str,
) -> Result<Vec<String>> {
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
pub(crate) fn write_json_artifact<T: serde::Serialize>(
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
pub(crate) fn build_audit_report(
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

pub(crate) async fn run(subcmd: TestCommands, db: &state::Db) -> Result<()> {
    match subcmd {
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
                if let Some(reason) = plan.repair_reason() {
                    println!("  Repair:    {}", reason);
                }
                println!();
                for job in &plan.selected_jobs {
                    println!("  ✓ {}", job);
                }
            }

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
                let json_value = test_intel::nightly_report::explain_audit_json(&report);
                println!("{}", serde_json::to_string_pretty(&json_value)?);
            } else {
                print!("{}", test_intel::nightly_report::explain_audit(&report));
            }

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
            let diff_output = std::process::Command::new("git")
                .args(["diff", "--name-only", &base, &head])
                .output()?;
            let changed_paths: Vec<String> = String::from_utf8_lossy(&diff_output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();

            let plan = test_intel::planner::plan_tests(&changed_paths);

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
                        1,
                    );
                    (t.command.clone(), key)
                })
                .collect();

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
        _ => unreachable!("non-tail command routed through root test handler"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Choose/Impact helpers (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "test_back_choose.rs"]
mod test_back_choose;
pub(crate) use test_back_choose::{handle_choose, handle_impact};
