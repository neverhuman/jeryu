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

    /// Follow agent-run events with an optional resume cursor.
    Follow {
        /// Agent run id.
        run_id: String,
        /// Only print events after this sequence.
        #[arg(long, default_value_t = 0)]
        after_seq: u64,
        /// Maximum events to fetch in this poll.
        #[arg(long, default_value_t = 100)]
        limit: u64,
    },

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
    /// Workcell id to launch from.
    #[arg(long)]
    pub workcell_id: Option<String>,
    /// Runner epoch for the workcell.
    #[arg(long)]
    pub runner_epoch: Option<u64>,
    /// Selected repo root inside the held workcell.
    #[arg(long)]
    pub repo_root: Option<PathBuf>,
    /// Program to execute inside the repo root.
    #[arg(long)]
    pub program: Option<String>,
    /// Extra program arguments.
    #[arg(long = "arg")]
    pub args: Vec<String>,
    /// I/O mode.
    #[arg(long, default_value = "pty")]
    pub io_mode: String,
    /// Managed repository as owner/name for fail-closed planning.
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
    /// Repository owner.
    #[arg(long)]
    pub owner: Option<String>,
    /// Repository name.
    #[arg(long)]
    pub repo: Option<String>,
    /// PR author login.
    #[arg(long)]
    pub author: Option<String>,
    /// Target branch.
    #[arg(long)]
    pub target_branch: Option<String>,
}
