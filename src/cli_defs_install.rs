use clap::{Args, Subcommand};
use std::path::PathBuf;

use jeryu::install::{ColorMode, InteractiveMode, PathMode};

use super::super::parse_expanded_path;

#[derive(Args)]
pub(crate) struct InstallCommand {
    #[arg(
        long,
        global = true,
        default_value = "~/.jeryu/bin",
        value_parser = parse_expanded_path
    )]
    pub prefix: PathBuf,
    #[arg(long, global = true, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub yes: bool,
    #[arg(long, global = true, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,
    #[arg(long, global = true, value_enum, default_value_t = InteractiveMode::Auto)]
    pub interactive: InteractiveMode,
    #[arg(long, global = true, value_enum, default_value_t = PathMode::Advise)]
    pub path_mode: PathMode,
    #[arg(long, global = true, default_value_t = false)]
    pub verbose: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub install_deps: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub allow_sudo: bool,
    #[command(subcommand)]
    pub action: Option<InstallActionCommands>,
}

#[derive(Subcommand)]
pub(crate) enum InstallActionCommands {
    Guided,
    Doctor,
    Smoke,
    Server,
    Uninstall,
    RenderDemo {
        #[arg(long, value_parser = parse_expanded_path)]
        output: PathBuf,
        #[arg(long, value_parser = parse_expanded_path)]
        png: Option<PathBuf>,
    },
}
