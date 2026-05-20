use clap::{Args, Subcommand};
use std::path::PathBuf;

use jeryu::install::{ColorMode, InteractiveMode, PathMode};
use jeryu::remote::ServiceMode;

use super::{
    AgentCommands, CacheCommands, HostCommands, JobCommands, LocalCommands, PipelineCommands,
    PolicyCommands, PoolCommands, ReleaseCommands, RepoCommands, SecretsCommands, SettingsCommands,
    TestCommands, parse_exec_script_path, parse_expanded_path,
};

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

#[derive(Args)]
pub(crate) struct RemoteCommand {
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

#[derive(Subcommand)]
pub(crate) enum BugCommands {
    #[command(subcommand)]
    Project(BugProjectCommands),
    Submit {
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value = "auto")]
        source: String,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        publish: bool,
        #[arg(long)]
        idempotency_key: Option<String>,
    },
    List {
        #[arg(long, default_value = "all")]
        project: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value = "rank")]
        sort: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Show {
        bug_id: String,
        #[arg(long, default_value_t = false)]
        history: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Triage {
        bug_id: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        severity: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        component: Option<String>,
        #[arg(long)]
        owner: Option<String>,
    },
    Link {
        bug_id: String,
        other_id: String,
        #[arg(long)]
        kind: String,
    },
    Ready {
        #[arg(long, default_value = "all")]
        project: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(subcommand)]
    Attempt(BugAttemptCommands),
    Sync {
        bug_id: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        provider: String,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum BugProjectCommands {
    Add {
        alias: String,
        #[arg(long)]
        repo_root: PathBuf,
        #[arg(long)]
        repo_slug: String,
        #[arg(long, default_value = "local")]
        provider: String,
        #[arg(long)]
        provider_project_id: Option<String>,
        #[arg(long, default_value = "main")]
        default_branch: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Show {
        alias: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Link {
        source: String,
        target: String,
        #[arg(long)]
        kind: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum BugAttemptCommands {
    Start {
        bug_id: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        sandbox_path: Option<PathBuf>,
    },
    Fail {
        bug_id: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        ci_evidence: Option<String>,
    },
    Complete {
        bug_id: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        pr_url: Option<String>,
        #[arg(long)]
        head_sha: Option<String>,
        #[arg(long)]
        notes: Option<String>,
    },
}

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
        #[arg(long, default_value = "2")]
        project_id: i64,
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
    Mcp(McpCommands),
    Next {
        #[arg(long, default_value = "2")]
        project_id: i64,
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

// ---------------------------------------------------------------------------
// Auxiliary command enums (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "cli_defs_aux.rs"]
mod cli_defs_aux;
pub(crate) use cli_defs_aux::*;
