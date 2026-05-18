use clap::Subcommand;

use super::Commands;
use super::parse_exec_script_path;

#[derive(Subcommand)]
pub(crate) enum ActionCommands {
    /// List all registered actions.
    List {
        /// Output as JSON (for agent consumption).
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Clone)]
pub(crate) enum ExecCommands {
    /// Provide driver configuration to GitLab.
    Config,
    /// Prepare the execution environment (spin up container).
    Prepare,
    /// Run an execution stage (e.g., build_script, step_script).
    Run {
        /// Script path provided by GitLab Runner
        #[arg(value_parser = parse_exec_script_path)]
        script_path: String,
        /// Stage name
        stage: String,
    },
    /// Cleanup the execution environment.
    Cleanup,
}

pub(crate) fn exec_subcommand(command: &Commands) -> Option<ExecCommands> {
    match command {
        Commands::Exec(subcmd) => Some(subcmd.clone()), // allowlist: typed clap subcommand; invocations stay typed
        _ => None,
    }
}

#[derive(Subcommand)]
pub(crate) enum ServerHookCommands {
    /// Act as a pre-receive git server hook
    PreReceive,
}

#[derive(Subcommand)]
pub(crate) enum CapabilityCommands {
    /// Start the capability API server
    Serve { socket_path: String },
}

#[derive(Subcommand)]
pub(crate) enum McpCommands {
    /// Start the MCP server over stdio.
    Serve,
    /// Start the MCP server over Streamable HTTP on the configured loopback bind.
    ServeHttp,
    /// Print the MCP tool manifest as JSON.
    Tools {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}
