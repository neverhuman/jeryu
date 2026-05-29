use anyhow::Result;
use jeryu::{docker, runner_fleet, state};

use crate::cli::{RunnerCommands, RunnerFleetCommands};

pub(crate) async fn execute_runner_commands(subcmd: RunnerCommands) -> Result<i32> {
    match subcmd {
        RunnerCommands::Fleet(cmd) => execute_runner_fleet_commands(cmd).await,
    }
}

async fn execute_runner_fleet_commands(cmd: RunnerFleetCommands) -> Result<i32> {
    let db = state::Db::open().await?;
    let docker = docker::DockerCtl::connect()?;
    match cmd {
        RunnerFleetCommands::Doctor { json } => {
            let report = runner_fleet::build_runner_fleet_report(&db, &docker).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_runner_fleet_report(&report);
            }
            Ok(if report.ok { 0 } else { 1 })
        }
        RunnerFleetCommands::Repair { preview, yes, json } => {
            if yes && preview {
                anyhow::bail!("choose either --preview or --yes, not both");
            }
            if !yes && !preview {
                anyhow::bail!("runner fleet repair requires --preview or --yes");
            }
            let report = runner_fleet::repair_runner_fleet(&db, &docker, yes).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "runner fleet repair {}: {} action(s)",
                    if report.executed {
                        "executed"
                    } else if report.blocked_reason.is_some() {
                        "blocked"
                    } else {
                        "preview"
                    },
                    report.actions.len()
                );
                if let Some(reason) = &report.blocked_reason {
                    println!("  blocked: {reason}");
                }
                for action in &report.actions {
                    println!(
                        "  - {} node={} manager={} container={}{}",
                        action.action,
                        action.node_alias,
                        action.manager_id.as_deref().unwrap_or("-"),
                        action.container_id.as_deref().unwrap_or("-"),
                        if action.executed { " executed" } else { "" }
                    );
                }
                print_runner_fleet_report(&report.doctor);
            }
            Ok(if report.doctor.ok { 0 } else { 1 })
        }
    }
}

fn print_runner_fleet_report(report: &runner_fleet::RunnerFleetReport) {
    println!("━━━ jeryu runner fleet doctor ━━━");
    println!("  Status: {}", if report.ok { "ok" } else { "blocked" });
    println!("  Partial: {}", report.partial);
    println!(
        "  DB/live: {}/{} active/running, containers seen={}",
        report.totals.db_active_managers,
        report.totals.live_running_containers,
        report.totals.containers_seen
    );
    println!(
        "  Drift: rehydratable={} orphaned={} stale={} missing-label={} over-capacity={} reserved-violations={}",
        report.totals.rehydratable,
        report.totals.orphaned,
        report.totals.stale_created,
        report.totals.missing_label,
        report.totals.over_capacity,
        report.totals.reserved_node_violation
    );
    for node in &report.nodes {
        println!(
            "  {:<10} db={} live={} expected={} max={}{}",
            node.node_alias,
            node.db_active_managers,
            node.live_running_containers,
            node.expected_managers,
            node.max_managers,
            if node.partial { " partial" } else { "" }
        );
    }
    if !report.issues.is_empty() {
        println!();
        println!("  Findings:");
        for issue in &report.issues {
            println!(
                "    [{}] {} {}: {}",
                issue.severity,
                issue.code,
                issue.node_alias.as_deref().unwrap_or("-"),
                issue.message
            );
        }
    }
}
