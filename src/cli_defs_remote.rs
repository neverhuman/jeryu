use clap::{Args, Subcommand};
use std::path::PathBuf;

use jeryu::remote::ServiceMode;

use super::super::parse_expanded_path;

#[derive(Args)]
pub(crate) struct RemoteCommand {
    #[arg(long, global = true, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub yes: bool,
    #[arg(long, global = true, value_enum, default_value_t = jeryu::install::ColorMode::Auto)]
    pub color: jeryu::install::ColorMode,
    #[arg(long, global = true, value_enum, default_value_t = jeryu::install::InteractiveMode::Auto)]
    pub interactive: jeryu::install::InteractiveMode,
    #[arg(long, global = true, value_enum, default_value_t = ServiceMode::Auto)]
    pub service_mode: ServiceMode,
    #[arg(long, global = true, default_value_t = false)]
    pub verbose: bool,
    #[command(subcommand)]
    pub action: RemoteActionCommands,
}

#[derive(Subcommand)]
pub(crate) enum RemoteActionCommands {
    Install {
        target: String,
        #[arg(long)]
        alias: Option<String>,
        #[arg(long, default_value_t = false)]
        setup_key: bool,
        #[arg(long, value_parser = parse_expanded_path)]
        identity: Option<PathBuf>,
    },
    #[clap(name = concat!("up", "date"))]
    Refresh {
        alias: String,
    },
    Doctor {
        alias: String,
    },
    Status {
        alias: String,
    },
    Logs {
        alias: String,
    },
    Restart {
        alias: String,
    },
    Stop {
        alias: String,
    },
    Start {
        alias: String,
    },
    Ssh {
        alias: String,
    },
    Run {
        alias: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    Tunnel {
        alias: String,
    },
    Uninstall {
        alias: String,
    },
}
