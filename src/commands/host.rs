use crate::cli::HostCommands;
use anyhow::Result;
use jeryu::{cache, reclaim, state};

pub(crate) async fn execute_host_commands(subcmd: HostCommands) -> Result<i32> {
    match subcmd {
        HostCommands::StorageAudit => {
            reclaim::run_storage_audit().await?;
        }
        HostCommands::Doctor { json } => {
            let report = cache::SmartCache::new(state::Db::open().await?)
                .host_doctor_report()
                .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                cache::print_host_doctor_report(&report);
            }
            if !report.ok {
                anyhow::bail!("host doctor found unhealthy CI host state");
            }
        }
        HostCommands::InstallGcTimer { allow_sudo } => {
            jeryu::host::install_gc_timer(allow_sudo).await?;
        }
        HostCommands::InstallGcdService { allow_sudo } => {
            jeryu::host::install_gcd_service(allow_sudo).await?;
        }
        HostCommands::Reclaim { mode, plan, apply } => {
            if mode != "aggressive" {
                anyhow::bail!("Only --mode aggressive is currently supported for host reclaim.");
            }
            if !plan && !apply {
                anyhow::bail!("You must specify either --plan or --apply.");
            }
            reclaim::run_aggressive_reclaim(apply).await?;
        }
    }
    Ok(0)
}
