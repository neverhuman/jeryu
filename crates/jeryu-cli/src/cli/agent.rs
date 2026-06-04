//! Agent-edit CLI command grammar.

use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Agent-edit command group.
#[derive(Debug, Subcommand)]
pub enum AgentCommands {
    /// Import or inspect portable native CLI auth.
    #[command(subcommand)]
    Auth(AgentAuthCommands),

    /// Start an agent-edit run.
    Run(AgentRunArgs),

    /// Show an agent-edit run.
    Status {
        /// Agent run id.
        run_id: String,
    },

    /// Send control to an agent-edit run.
    Control(AgentControlArgs),

    /// Export a completed agent-edit run as a PR.
    #[command(name = "export-pr")]
    ExportPr(AgentExportPrArgs),
}

/// Auth subcommands.
#[derive(Debug, Subcommand)]
pub enum AgentAuthCommands {
    /// Import portable auth from the host into Jeryu-owned storage.
    Import {
        /// Host tool whose portable auth should be imported.
        #[arg(long = "from-host")]
        from_host: AgentToolArg,
    },

    /// Check imported portable auth.
    Doctor {
        /// Tool to check.
        tool: AgentToolArg,
    },
}

/// Native CLI kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentToolArg {
    /// Codex CLI.
    Codex,
    /// Claude CLI.
    Claude,
    /// Jekko CLI.
    Jekko,
}

/// Start-run arguments.
#[derive(Debug, Args)]
pub struct AgentRunArgs {
    /// Managed repository as owner/name.
    #[arg(long)]
    pub repo: String,
    /// Agent tool to run.
    #[arg(long)]
    pub agent: AgentToolArg,
    /// Model name to pass through.
    #[arg(long)]
    pub model: String,
    /// Reasoning effort label.
    #[arg(long, default_value = "xhigh")]
    pub effort: String,
    /// File containing the task prompt.
    #[arg(long = "task-file")]
    pub task_file: PathBuf,
    /// Base ref for the workcell.
    #[arg(long, default_value = "main")]
    pub base_ref: String,
}

/// Control arguments.
#[derive(Debug, Args)]
pub struct AgentControlArgs {
    /// Agent run id.
    pub run_id: String,
    /// Text to send to stdin.
    #[arg(long = "stdin")]
    pub stdin_text: Option<String>,
    /// Send an interrupt.
    #[arg(long, default_value_t = false)]
    pub interrupt: bool,
    /// Terminate the run.
    #[arg(long, default_value_t = false)]
    pub terminate: bool,
}

/// Export-PR arguments.
#[derive(Debug, Args)]
pub struct AgentExportPrArgs {
    /// Agent run id.
    pub run_id: String,
    /// Pull request title.
    #[arg(long)]
    pub title: String,
    /// Optional pull request body.
    #[arg(long)]
    pub body: Option<String>,
}
