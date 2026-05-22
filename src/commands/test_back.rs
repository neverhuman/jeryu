use crate::cli::TestCommands;
use anyhow::Result;
use jeryu::{state, test_intel};
#[path = "test_back_support.rs"]
pub(crate) mod test_back_support;
use self::test_back_support::build_audit_report;
#[allow(unused_imports)]
pub(crate) use test_back_support::{git_diff_changed_paths, write_json_artifact};

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
            test_back_support::handle_cache_status_command(base, head, json)?;
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
