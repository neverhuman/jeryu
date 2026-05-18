use anyhow::Result;

use crate::cli::ExecCommands;

pub(crate) async fn execute_exec_commands(subcmd: ExecCommands) -> Result<i32> {
    match subcmd {
        ExecCommands::Config => {
            jeryu::exec::run_config()?;
        }
        ExecCommands::Prepare => {
            jeryu::exec::run_prepare().await?;
        }
        ExecCommands::Run { script_path, stage } => {
            jeryu::exec::run_stage(&script_path, &stage).await?;
        }
        ExecCommands::Cleanup => {
            jeryu::exec::run_cleanup().await?;
        }
    }

    Ok(0)
}
