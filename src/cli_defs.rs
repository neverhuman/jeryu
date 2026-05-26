use clap::Subcommand;
use std::path::PathBuf;

use super::{
    AgentCommands, CacheCommands, HostCommands, JobCommands, LocalCommands, PipelineCommands,
    PolicyCommands, PoolCommands, ReleaseCommands, SecretsCommands, SettingsCommands, TestCommands,
};

#[path = "cli_defs_install.rs"]
mod cli_defs_install;
pub(crate) use cli_defs_install::{InstallActionCommands, InstallCommand};

#[path = "cli_defs_remote.rs"]
mod cli_defs_remote;
pub(crate) use cli_defs_remote::{RemoteActionCommands, RemoteCommand};
#[derive(Subcommand)]
pub(crate) enum Commands {
    Init,
    #[command(hide = true)]
    Bootstrap,
    Serve,
    Install(InstallCommand),
    Remote(RemoteCommand),
    Tui {
        #[arg(long, default_value_t = false)]
        once: bool,
        #[arg(long, default_value_t = false)]
        demo: bool,
        #[arg(long, default_value_t = false)]
        capture: bool,
        #[arg(long, default_value_t = false)]
        screenshot: bool,
        #[arg(long, default_value = "jobs")]
        tab: String,
        #[arg(long, default_value = "paper/assets/jeryu-tui.png")]
        output: PathBuf,
        #[arg(long, default_value_t = 140)]
        width: u16,
        #[arg(long, default_value_t = 44)]
        height: u16,
        #[arg(long, default_value_t = 1100)]
        screenshot_hold_ms: u64,
    },
    Down,
    Git {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Save {
        message: String,
    },
    Sync,
    Undo,
    System,
    Status,
    #[command(subcommand)]
    Pool(PoolCommands),
    #[command(subcommand)]
    Job(JobCommands),
    #[command(subcommand)]
    Pipeline(PipelineCommands),
    #[command(subcommand)]
    Cache(CacheCommands),
    #[command(subcommand)]
    Local(LocalCommands),
    Logs {
        manager_id: String,
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
    },
    #[command(subcommand)]
    Agent(AgentCommands),
    #[command(subcommand)]
    Settings(SettingsCommands),
    #[command(subcommand)]
    Test(TestCommands),
    #[command(subcommand)]
    Release(ReleaseCommands),
    #[command(subcommand)]
    Secrets(SecretsCommands),
    Progress {
        #[arg(long)]
        project_id: Option<i64>,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(subcommand)]
    Repo(RepoCommands),
    #[command(subcommand)]
    Bug(BugCommands),
    #[command(subcommand)]
    Policy(PolicyCommands),
    #[command(subcommand)]
    Host(HostCommands),
    #[command(subcommand, hide = true)]
    Exec(ExecCommands), // allowlist: typed clap subcommand; invocations stay typed
    #[command(subcommand, hide = true)]
    ServerHook(ServerHookCommands),
    #[command(subcommand, hide = true)]
    Capability(CapabilityCommands),
    #[command(subcommand)]
    Node(NodeCommands),
    #[command(subcommand)]
    Mcp(McpCommands),
    Next {
        #[arg(long)]
        project_id: Option<i64>,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
    },
    ExplainBlocker {
        entity_type: String,
        entity_id: i64,
    },
    #[command(name = "action", subcommand)]
    Action(ActionCommands),
}

#[path = "cli_defs_commands.rs"]
mod cli_defs_commands;
pub(crate) use cli_defs_commands::{
    BugAttemptCommands, BugCommands, BugProjectCommands, RepoCommands, RepoFleetCommands,
    RepoHookCommands, RepoStandardCommand, RepoStandardCommands,
};

#[path = "cli_defs_node.rs"]
mod cli_defs_node;
pub(crate) use cli_defs_node::NodeCommands;

// ---------------------------------------------------------------------------
// Auxiliary command enums (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "cli_defs_aux.rs"]
mod cli_defs_aux;
pub(crate) use cli_defs_aux::*;
