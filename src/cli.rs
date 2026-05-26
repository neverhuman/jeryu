//! Owner: CLI Definitions
//! Proof: `cargo check -p jeryu`
//! Invariants: All types are pub(crate); main.rs is the only consumer
//!
//! Pure data: clap struct/enum definitions for the `jeryu` CLI.
//! No logic lives here — dispatch is in `dispatch.rs`.

use clap::Parser;
use std::path::PathBuf;

use jeryu::exec;

#[path = "cli_runtime_commands.rs"]
mod cli_runtime_commands;
pub(crate) use cli_runtime_commands::*;

#[path = "cli_test_commands.rs"]
mod cli_test_commands;
pub(crate) use cli_test_commands::*;

#[cfg(test)]
#[path = "cli_tests.rs"]
mod cli_tests;

#[path = "cli_defs.rs"]
mod cli_defs;
pub(crate) use cli_defs::*;

#[derive(Parser)]
#[command(
    name = "jeryu",
    version = env!("JERYU_FULL_VERSION"),
    about = "Git-compatible version control layer for the AI era"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

fn parse_expanded_path(input: &str) -> Result<PathBuf, String> {
    Ok(jeryu::install::expand_tilde(input))
}

fn parse_exec_script_path(input: &str) -> Result<String, String> {
    exec::validate_script_path(input)
        .map(|_| input.to_string())
        .map_err(|err| err.to_string())
}

pub fn infer_repo_name() -> String {
    match std::env::current_dir() {
        Ok(path) => match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => "jeryu".to_string(),
        },
        Err(_) => "jeryu".to_string(),
    }
}
