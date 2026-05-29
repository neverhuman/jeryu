use anyhow::Result;

use crate::cli::CiCommands;

pub(crate) async fn execute_ci_commands(cmd: CiCommands) -> Result<i32> {
    match cmd {
        CiCommands::Doctor { repo, json } => {
            let report = jeryu::ci_policy::doctor_repo(&repo)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_ci_policy_report(&report);
            }
            Ok(if report.ok { 0 } else { 1 })
        }
        CiCommands::FleetDoctor { json } => {
            let report = jeryu::ci_policy::doctor_default_fleet()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("━━━ jeryu ci fleet doctor ━━━");
                println!("  Repos: {}", report.repos.len());
                println!("  Status: {}", if report.ok { "ok" } else { "blocked" });
                for repo in &report.repos {
                    println!("  {} {}", if repo.ok { "ok" } else { "blocked" }, repo.repo);
                }
            }
            Ok(if report.ok { 0 } else { 1 })
        }
        CiCommands::Template { profile } => {
            if profile != "rust" {
                anyhow::bail!("only --profile rust is currently supported");
            }
            print!("{}", jeryu::ci_policy::render_rust_gitlab_ci_template());
            Ok(0)
        }
    }
}

fn print_ci_policy_report(report: &jeryu::ci_policy::CiPolicyReport) {
    println!("━━━ jeryu ci doctor ━━━");
    println!("  Repo:   {}", report.repo);
    println!("  Status: {}", if report.ok { "ok" } else { "blocked" });
    println!("  Files:  {}", report.files_checked.len());
    if !report.findings.is_empty() {
        println!();
        println!("  Findings:");
        for finding in &report.findings {
            let line = finding
                .line
                .map(|line| format!(":{line}"))
                .unwrap_or_default();
            println!(
                "    [{}] {} {}{}: {}",
                finding.severity, finding.code, finding.path, line, finding.message
            );
        }
    }
}
