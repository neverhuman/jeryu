use anyhow::Result;
use clap::Subcommand;
use jeryu::pool;
use jeryu::pool_service::PoolListRow;

#[derive(Subcommand)]
pub(crate) enum PoolCommands {
    /// List all pools and their managers.
    List,
    /// Diagnose runner pool policy, runtime, and topology drift.
    Doctor {
        /// Output as JSON for scripting/agent consumption.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Repair standard runner policy and topology drift.
    Repair {
        /// Confirm repairs.
        #[arg(long, default_value_t = false)]
        yes: bool,
        /// Delete outdated standard GitLab runner registrations that are not referenced by a pool.
        #[arg(long, default_value_t = false)]
        prune_outdated: bool,
        /// Output as JSON for scripting/agent consumption.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Scale a pool to N managers.
    Scale { name: String, count: usize },
    /// Pause a pool (stop accepting new jobs).
    Pause { name: String },
    /// Resume a paused pool.
    Resume { name: String },
    /// Drain a pool: pause, wait for jobs to finish, stop managers.
    Drain { name: String },
    /// Drain and remove a pool plus its GitLab runner registration.
    #[clap(name = "delete")]
    Remove { name: String },
    /// Rotate the auth token for a pool.
    RotateToken { name: String },
}

pub(crate) async fn execute_pool_commands(
    service: &jeryu::pool_service::PoolService,
    subcmd: PoolCommands,
) -> Result<()> {
    match subcmd {
        PoolCommands::List => {
            print_pool_list(&service.list().await?);
        }
        PoolCommands::Doctor { json } => {
            let report = service.doctor().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_pool_doctor_report(&report);
            }
        }
        PoolCommands::Repair {
            yes,
            prune_outdated,
            json,
        } => {
            if !yes {
                anyhow::bail!("refusing to repair pools without --yes");
            }
            let report = service
                .repair(pool::PoolRepairOptions { prune_outdated })
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("pool repair: {} action(s)", report.actions.len());
                for action in &report.actions {
                    println!("  - {action}");
                }
                print_pool_doctor_report(&report.doctor);
            }
        }
        PoolCommands::Scale { name, count } => {
            let started = service.scale(&name, count).await?;
            println!(
                "✅ Pool '{}' scaled to {} (started {} new)",
                name, count, started
            );
        }
        PoolCommands::Pause { name } => {
            service.pause(&name).await?;
            println!("⏸  Pool '{}' paused", name);
        }
        PoolCommands::Resume { name } => {
            service.resume(&name).await?;
            println!("▶  Pool '{}' resumed", name);
        }
        PoolCommands::Drain { name } => {
            service.drain(&name).await?;
            println!("✅ Pool '{}' drained", name);
        }
        PoolCommands::Remove { name } => {
            service.delete(&name).await?;
            println!("✅ Pool '{}' deleted", name);
        }
        PoolCommands::RotateToken { name } => {
            let new_token = service.rotate_token(&name).await?;
            println!(
                "🔑 Pool '{}' token rotated: {}...{}",
                name,
                &new_token[..8],
                &new_token[new_token.len() - 4..]
            );
        }
    }
    Ok(())
}

fn print_pool_list(rows: &[PoolListRow]) {
    println!(
        "{:<15} {:<8} {:<10} {:<8} {:<12} {:<8}",
        "NAME", "PAUSED", "EXECUTOR", "WARM", "LIVE/DB/MAX", "RUNNER"
    );
    for row in rows {
        println!(
            "{:<15} {:<8} {:<10} {:<8} {:<12} {:<8}",
            row.name.as_str(),
            if row.paused { "yes" } else { "no" },
            row.executor.as_str(),
            row.min_warm,
            row.manager_status(),
            row.gitlab_runner_id,
        );
    }
}

fn print_pool_doctor_report(report: &pool::PoolDoctorReport) {
    println!("━━━ jeryu pool doctor ━━━");
    println!("  Status: {}", if report.ok { "ok" } else { "blocked" });
    println!("  Issues: {}", report.issues.len());
    if let Some(topology) = &report.topology {
        println!(
            "  Standard topology: active={} desired={}",
            topology.active_total, topology.desired_total
        );
        for entry in &topology.entries {
            println!(
                "    {:<10} active={:<3} desired={:<3} delta={}",
                entry.node_alias, entry.active, entry.desired, entry.delta
            );
        }
    }
    if !report.reserved_nodes.is_empty() {
        println!("  Reserved nodes:");
        for node in &report.reserved_nodes {
            println!(
                "    {:<10} active={:<3} desired={:<3} {}",
                node.node_alias, node.active, node.desired, node.reason
            );
        }
    }
    if !report.issues.is_empty() {
        println!();
        println!("  Findings:");
        for issue in &report.issues {
            let scope = issue
                .pool
                .as_deref()
                .or(issue.node_alias.as_deref())
                .or(issue.manager_id.as_deref())
                .unwrap_or("-");
            println!(
                "    [{}] {} {}: {}",
                issue.severity, issue.code, scope, issue.message
            );
        }
    }
}
