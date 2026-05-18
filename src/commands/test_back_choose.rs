use anyhow::Result;
use jeryu::test_intel;
use std::path::PathBuf;

use super::git_diff_changed_paths;
use super::write_json_artifact;

pub(crate) async fn handle_impact(
    base: String,
    head: String,
    repo_root: PathBuf,
    json: bool,
) -> Result<()> {
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
        let release_impacting = value["release_impacting"].as_bool().unwrap_or(true);
        let full_build_required = value["full_build_required"].as_bool().unwrap_or(true);
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
    Ok(())
}

#[allow(clippy::too_many_arguments)] // CLI flag passthrough; flat by design
pub(crate) fn handle_choose(
    base: String,
    head: String,
    cwd: PathBuf,
    explain: bool,
    json: bool,
    emit_gitlab: Option<PathBuf>,
    emit_plan: Option<PathBuf>,
    emit_receipt: Option<PathBuf>,
) -> Result<()> {
    let changed_paths = git_diff_changed_paths(&cwd, &base, &head)?;
    let plan = test_intel::planner::plan_tests(&changed_paths);
    let receipt = plan.receipt(Some(&base), Some(&head));
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
        if let Some(reason) = plan.repair_reason() {
            println!("  Repair:     {}", reason);
        }
        println!();
        for test in &plan.selected_tests {
            println!("  ✓ [{}] {}", test.subsystem, test.command);
        }
    }
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
    Ok(())
}
