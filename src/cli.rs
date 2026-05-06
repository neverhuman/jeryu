//! Owner: CLI Definitions
//! Proof: `cargo check -p jeryu`
//! Invariants: All types are pub(crate); main.rs is the only consumer
//!
//! Pure data: clap struct/enum definitions for the `jeryu` CLI.
//! No logic lives here — dispatch is in `dispatch.rs`.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use jeryu::install::{ColorMode, InteractiveMode, PathMode};
use jeryu::remote::ServiceMode;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "jeryu",
    version,
    about = "Git-compatible version control layer for the AI era"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

fn parse_expanded_path(input: &str) -> Result<PathBuf, String> {
    Ok(jeryu::install::expand_tilde(input))
}

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
    /// Inspect the current install target without mutating the machine.
    Doctor,
    /// Install into a throwaway prefix and verify the result.
    Smoke,
    /// Install the binary, verify Docker, and run `jeryu init`.
    Server,
    /// Remove the installed binary.
    Uninstall,
    /// Render the deterministic install GIF.
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
    /// Provision a remote host and save its metadata under ~/.jeryu/remotes.
    Install {
        target: String,
        #[arg(long)]
        alias: Option<String>,
        #[arg(long, default_value_t = false)]
        setup_key: bool,
        #[arg(long, value_parser = parse_expanded_path)]
        identity: Option<PathBuf>,
    },
    /// Re-upload the current binary and refresh the remote service.
    Update { alias: String },
    /// Inspect remote health.
    Doctor { alias: String },
    /// Show remote service status.
    Status { alias: String },
    /// Tail remote logs.
    Logs { alias: String },
    /// Restart the remote service.
    Restart { alias: String },
    /// Stop the remote service.
    Stop { alias: String },
    /// Start the remote service.
    Start { alias: String },
    /// Open an interactive SSH session.
    Ssh { alias: String },
    /// Run a remote command through the installed binary.
    Run {
        alias: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Open the standard SSH port tunnels.
    Tunnel { alias: String },
    /// Remove the remote binary and service.
    Uninstall { alias: String },
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Initialize the entire jeryu environment (GitLab + DB + Runners).
    Init,

    /// Alias for init.
    #[command(hide = true)]
    Bootstrap,

    /// Start all pools at their min_warm and run the background daemon.
    Serve,

    /// Local binary install, validation, and server bootstrap.
    Install(InstallCommand),

    /// Remote SSH setup and day-two management.
    Remote(RemoteCommand),

    /// Launch the interactive terminal UI.
    Tui {
        /// Render one frame and exit. Intended for CI smoke checks.
        #[arg(long, default_value_t = false)]
        once: bool,
        /// Render one deterministic PNG screenshot and exit.
        #[arg(long, default_value_t = false)]
        capture: bool,
        /// Render one deterministic screenshot from a real terminal session.
        #[arg(long, default_value_t = false)]
        screenshot: bool,
        /// TUI tab to render when capturing: mission, release, jobs, agents, tests, pools, cache, evidence, secrets.
        #[arg(long, default_value = "jobs")]
        tab: String,
        /// Output path for --capture.
        #[arg(long, default_value = "paper/assets/jeryu-tui.png")]
        output: PathBuf,
        /// Capture width in terminal cells.
        #[arg(long, default_value_t = 140)]
        width: u16,
        /// Capture height in terminal cells.
        #[arg(long, default_value_t = 44)]
        height: u16,
        /// Time to keep a screenshot session alive for external capture tooling.
        #[arg(long, default_value_t = 1100)]
        screenshot_hold_ms: u64,
    },

    /// Drain all managers and stop GitLab.
    Down,

    /// Passthrough command for git.
    Git {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Save your work (git add + commit).
    Save {
        /// The commit message
        message: String,
    },

    /// Sync with the remote (pull --rebase + push).
    Sync,

    /// Undo the last save (git reset HEAD~1 --soft).
    Undo,

    /// Show full JeRyu system status (formerly Status).
    System,

    /// Show git status (AI magic layer).
    Status,

    /// Pool management.
    #[command(subcommand)]
    Pool(PoolCommands),

    /// Job management.
    #[command(subcommand)]
    Job(JobCommands),

    /// Pipeline inspection.
    #[command(subcommand)]
    Pipeline(PipelineCommands),

    /// Cache management.
    #[command(subcommand)]
    Cache(CacheCommands),

    /// Local agent cache-aware command wrappers.
    #[command(subcommand)]
    Local(LocalCommands),

    /// Log inspection.
    Logs {
        /// Manager ID to tail logs from.
        manager_id: String,
        /// Number of lines to show.
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
    },

    /// Autonomous agent operations.
    #[command(subcommand)]
    Agent(AgentCommands),

    /// Repair or reset user settings.
    #[command(subcommand)]
    Settings(SettingsCommands),

    /// Run tests through the CI pipeline (agent-friendly).
    #[command(subcommand)]
    Test(TestCommands),

    /// Release monitoring and canary status.
    #[command(subcommand)]
    Release(ReleaseCommands),

    /// Vault-backed secret lifecycle and release handoff.
    #[command(subcommand)]
    Secrets(SecretsCommands),

    /// Show lane-aware CI and release progress for a ref.
    Progress {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Repo-local routing, agent surface, and generated ownership indexes.
    #[command(subcommand)]
    Repo(RepoCommands),

    /// Host node management and clean up.
    #[command(subcommand)]
    Host(HostCommands),

    /// Custom executor driver (invoked by gitlab-runner, not meant for humans).
    #[command(subcommand, hide = true)]
    Exec(ExecCommands),

    /// Git Server hook entrypoints for Admission Control.
    #[command(subcommand, hide = true)]
    ServerHook(ServerHookCommands),

    /// Capability API Server for Agent workers.
    #[command(subcommand, hide = true)]
    Capability(CapabilityCommands),

    /// MCP adapter for external coding agents.
    #[command(subcommand)]
    Mcp(McpCommands),

    /// Show the next highest-priority action for the current branch.
    Next {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
    },

    /// Explain why a job, release, or merge is blocked.
    ExplainBlocker {
        /// Entity type: job | release | merge
        entity_type: String,
        /// Entity ID (job_id, release attempt ID, or MR iid)
        entity_id: i64,
    },

    /// List all registered jeryu actions with risk tier and surfaces.
    #[command(name = "action", subcommand)]
    Action(ActionCommands),
}

#[derive(Subcommand)]
pub(crate) enum ActionCommands {
    /// List all registered actions.
    List {
        /// Output as JSON (for agent consumption).
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ExecCommands {
    /// Provide driver configuration to GitLab.
    Config,
    /// Prepare the execution environment (spin up container).
    Prepare,
    /// Run an execution stage (e.g., build_script, step_script).
    Run {
        /// Script path provided by GitLab Runner
        script_path: String,
        /// Stage name
        stage: String,
    },
    /// Cleanup the execution environment.
    Cleanup,
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn release_watch_accepts_ref_alias() {
        let cli = Cli::parse_from(["jeryu", "release", "watch", "--ref", "main"]);
        match cli.command {
            Commands::Release(ReleaseCommands::Watch { ref_name, .. }) => {
                assert_eq!(ref_name, "main");
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn release_watch_accepts_ref_name_spelling() {
        let cli = Cli::parse_from(["jeryu", "release", "watch", "--ref-name", "main"]);
        match cli.command {
            Commands::Release(ReleaseCommands::Watch { ref_name, .. }) => {
                assert_eq!(ref_name, "main");
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn install_render_demo_is_nested_under_install() {
        let cli = Cli::parse_from([
            "jeryu",
            "install",
            "render-demo",
            "--output",
            "assets/install-demo.gif",
        ]);
        match cli.command {
            Commands::Install(InstallCommand {
                action: Some(InstallActionCommands::RenderDemo { output, png }),
                ..
            }) => {
                assert!(output.ends_with("assets/install-demo.gif"));
                assert!(png.is_none());
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn install_smoke_accepts_common_flags_after_action() {
        let cli = Cli::parse_from(["jeryu", "install", "smoke", "--dry-run"]);
        match cli.command {
            Commands::Install(InstallCommand {
                dry_run,
                action: Some(InstallActionCommands::Smoke),
                ..
            }) => {
                assert!(dry_run);
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn install_accepts_new_ui_flags_before_action() {
        let cli = Cli::parse_from([
            "jeryu",
            "install",
            "--color",
            "always",
            "--interactive",
            "never",
            "--path-mode",
            "update",
            "--verbose",
            "doctor",
        ]);
        match cli.command {
            Commands::Install(InstallCommand {
                color,
                interactive,
                path_mode,
                verbose,
                action: Some(InstallActionCommands::Doctor),
                ..
            }) => {
                assert_eq!(color, ColorMode::Always);
                assert_eq!(interactive, InteractiveMode::Never);
                assert_eq!(path_mode, PathMode::Update);
                assert!(verbose);
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn remote_install_parses_alias_and_setup_key() {
        let cli = Cli::parse_from([
            "jeryu",
            "remote",
            "install",
            "xbabe1",
            "--alias",
            "lab",
            "--setup-key",
        ]);
        match cli.command {
            Commands::Remote(RemoteCommand {
                action:
                    RemoteActionCommands::Install {
                        target,
                        alias,
                        setup_key,
                        ..
                    },
                ..
            }) => {
                assert_eq!(target, "xbabe1");
                assert_eq!(alias.as_deref(), Some("lab"));
                assert!(setup_key);
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn remote_install_accepts_common_flags_after_action() {
        let cli = Cli::parse_from([
            "jeryu",
            "remote",
            "install",
            "xbabe1",
            "--dry-run",
            "--yes",
            "--setup-key",
        ]);
        match cli.command {
            Commands::Remote(RemoteCommand {
                dry_run,
                yes,
                action:
                    RemoteActionCommands::Install {
                        target, setup_key, ..
                    },
                ..
            }) => {
                assert_eq!(target, "xbabe1");
                assert!(dry_run);
                assert!(yes);
                assert!(setup_key);
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn remote_install_accepts_service_and_ui_flags() {
        let cli = Cli::parse_from([
            "jeryu",
            "remote",
            "--color",
            "never",
            "--interactive",
            "always",
            "--service-mode",
            "manual",
            "--verbose",
            "install",
            "xbabe1",
        ]);
        match cli.command {
            Commands::Remote(RemoteCommand {
                color,
                interactive,
                service_mode,
                verbose,
                action: RemoteActionCommands::Install { target, .. },
                ..
            }) => {
                assert_eq!(target, "xbabe1");
                assert_eq!(color, ColorMode::Never);
                assert_eq!(interactive, InteractiveMode::Always);
                assert_eq!(service_mode, ServiceMode::Manual);
                assert!(verbose);
            }
            _ => panic!("unexpected command parsed"),
        }
    }

    #[test]
    fn cli_help_excludes_removed_git_commands() {
        let subcommands: Vec<String> = Cli::command()
            .get_subcommands()
            .map(|subcommand| subcommand.get_name().to_string())
            .collect();

        assert!(!subcommands.iter().any(|name| name == "ship"));
        assert!(!subcommands.iter().any(|name| name == "mirror"));
    }
}

#[derive(Subcommand)]
pub(crate) enum PoolCommands {
    /// List all pools and their managers.
    List,
    /// Scale a pool to N managers.
    Scale { name: String, count: usize },
    /// Pause a pool (stop accepting new jobs).
    Pause { name: String },
    /// Resume a paused pool.
    Resume { name: String },
    /// Drain a pool: pause, wait for jobs to finish, stop managers.
    Drain { name: String },
    /// Drain and delete a pool plus its GitLab runner registration.
    Delete { name: String },
    /// Rotate the auth token for a pool.
    RotateToken { name: String },
}

#[derive(Subcommand)]
pub(crate) enum JobCommands {
    /// List jobs for a project.
    List {
        project_id: i64,
        #[arg(long, default_value = "running,pending")]
        status: String,
    },
    /// Show job trace (log output).
    Trace { project_id: i64, job_id: i64 },
    /// Trigger a manual job.
    Play { project_id: i64, job_id: i64 },
    /// Cancel a running job.
    Cancel { project_id: i64, job_id: i64 },
    /// Retry a failed job.
    Retry { project_id: i64, job_id: i64 },
    /// Explain the latest structured failure evidence for a job.
    Explain { project_id: i64, job_id: i64 },
    /// Clear all job and pipeline histories from the database.
    Clear,
}

#[derive(Subcommand)]
pub(crate) enum PipelineCommands {
    /// Explain blocking vs non-blocking state for a specific pipeline.
    Explain {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Diagnose active jobs, runner assignment, and stale trace symptoms.
    Doctor {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List all jobs with start/end/runtime fields and optionally ingest them.
    Jobs {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        ingest: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Persist all current GitLab job timings for a pipeline.
    Ingest {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
    },
    /// Cancel a superseded or unwanted pipeline.
    Cancel {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
    },
    /// Show historical slow CI jobs from the local jeryu timing ledger.
    Bottlenecks {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref")]
        ref_name: Option<String>,
        #[arg(long, default_value = "25")]
        limit: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum CacheCommands {
    /// Enable and configure Docker daemon for SmartCache registry mirror.
    Enable,
    /// Health-check proxy and registry reachability.
    Doctor,
    /// Show live SmartCache state and metrics.
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run garbage collection on the cache store.
    Gc {
        /// Preview actions without deleting anything.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Emit machine-readable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Preserve cache directories for running runner managers.
        /// Pass --keep-active-managers=false to evict active caches at emergency disk pressure.
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true, default_missing_value = "true", num_args = 0..=1)]
        keep_active_managers: bool,
        /// Only delete orphan manager caches older than this age, e.g. 12h or 2d.
        #[arg(long)]
        older_than: Option<String>,
        /// If total manager cache exceeds this budget, include all orphan caches as candidates.
        #[arg(long)]
        max_cache_gb: Option<f64>,
    },
}

#[derive(Subcommand)]
pub(crate) enum LocalCommands {
    /// Run cargo with jeryu-managed cache roots for a repository checkout.
    Cargo {
        /// Repository root to run cargo in.
        #[arg(long)]
        repo: PathBuf,
        /// Cargo arguments to forward after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        cargo_args: Vec<String>,
    },
    /// Print the cache-aware Cargo environment for a repository checkout.
    CargoEnv {
        /// Repository root to inspect.
        #[arg(long)]
        repo: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum AgentCommands {
    /// Spawn an autonomous agent on a project.
    Spawn {
        project_id: i64,
        /// Description of the task for the agent.
        #[arg(short, long)]
        task: String,
    },
    /// List active agents.
    List { project_id: i64 },
    /// Merge an MR only if the risk gate allows it.
    Merge {
        project_id: i64,
        mr_iid: i64,
        #[arg(long, default_value = "trusted")]
        trust_tier: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum SettingsCommands {
    /// Validate settings and repair a corrupt file backup if present.
    Repair,
    /// Reset `~/.jeryu/settings.json` to defaults.
    Reset {
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum TestCommands {
    /// Run a single test command through a CI pipeline.
    Run {
        /// The test command to execute.
        #[arg(short, long)]
        command: String,
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long, default_value = "rust:1.92.0")]
        image: String,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
        #[arg(long)]
        force: bool,
    },
    /// Preview the inferred runner class and timeout for a command.
    Plan {
        #[arg(short, long)]
        command: String,
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long, default_value = "rust:1.92.0")]
        image: String,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
    },
    /// Run multiple test commands in parallel through separate pipelines.
    Batch {
        #[arg(short = 'c', long = "command", required = true)]
        commands: Vec<String>,
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long, default_value = "rust:1.92.0")]
        image: String,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
        #[arg(long, default_value = "3")]
        max_parallel: usize,
        #[arg(long)]
        force: bool,
    },

    /// Show results of all jobs in a pipeline.
    Results {
        pipeline_id: i64,
        #[arg(long, default_value = "2")]
        project_id: i64,
    },
    /// Retry a specific failed job by name.
    Retry {
        pipeline_id: i64,
        job_name: String,
        #[arg(long, default_value = "2")]
        project_id: i64,
    },
    /// Show only failed jobs from a pipeline with their traces.
    Failed {
        pipeline_id: i64,
        #[arg(long, default_value = "2")]
        project_id: i64,
    },
    /// Ask the checked-out project which CI jobs and release gates a diff needs.
    Impact {
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Smart test selection: compute the minimal test plan for a diff.
    Select {
        /// Base ref (e.g. origin/main, HEAD~1, SHA).
        #[arg(long, default_value = "origin/main")]
        base: String,
        /// Head ref (e.g. HEAD, SHA).
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Repository root to resolve changed files.
        #[arg(long)]
        repo_root: Option<PathBuf>,
        /// Print the plan explanation.
        #[arg(long, default_value_t = false)]
        explain: bool,
        /// Emit raw JSON plan.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Write the generated GitLab child pipeline YAML to this path.
        #[arg(long)]
        emit_gitlab: Option<PathBuf>,
        /// Write the JSON plan to this path.
        #[arg(long)]
        emit_plan: Option<PathBuf>,
        /// Write the VTI proof receipt JSON to this path.
        #[arg(long)]
        emit_receipt: Option<PathBuf>,
    },
    /// Explain a test plan (from JSON file or the last computed plan).
    ExplainPlan {
        /// Path to a JSON test plan file.
        plan_path: PathBuf,
    },
    /// Smart test selection for an external workspace with a `.jeryu/testmap.toml`.
    SelectExternal {
        /// Base ref (e.g. origin/main, HEAD~1, SHA).
        #[arg(long, default_value = "origin/main")]
        base: String,
        /// Head ref (e.g. HEAD, SHA).
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Path to the external workspace root (must contain .jeryu/testmap.toml).
        #[arg(long)]
        workspace: PathBuf,
        /// Print the plan explanation.
        #[arg(long, default_value_t = false)]
        explain: bool,
        /// Emit raw JSON plan.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Write the generated GitLab child pipeline YAML to this path.
        #[arg(long)]
        emit_gitlab: Option<PathBuf>,
        /// Write the JSON plan to this path.
        #[arg(long)]
        emit_plan: Option<PathBuf>,
        /// Write JSON metadata for jobs omitted by VTI to this path.
        #[arg(long)]
        emit_skipped: Option<PathBuf>,
    },
    /// Audit VTI accuracy: compare full test results against what VTI would have selected.
    Audit {
        /// Comma-separated list of changed paths.
        #[arg(long)]
        changed: String,
        /// Comma-separated list of failed test names.
        #[arg(long, default_value = "")]
        failed: String,
        /// Comma-separated list of all test names.
        #[arg(long)]
        all_tests: String,
        /// The SHA this audit covers.
        #[arg(long, default_value = "HEAD")]
        sha: String,
        /// Emit JSON output.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// The optional workspace path if running externally.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Learn from an audit report and suggest rule improvements.
    Learn {
        /// Comma-separated list of changed paths.
        #[arg(long)]
        changed: String,
        /// Comma-separated list of failed test names.
        #[arg(long, default_value = "")]
        failed: String,
        /// Comma-separated list of all test names.
        #[arg(long)]
        all_tests: String,
        /// The SHA this learning covers.
        #[arg(long, default_value = "HEAD")]
        sha: String,
        /// Emit JSON output.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// The optional workspace path if running externally.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Show cache status for test commands against the current source state.
    CacheStatus {
        /// Base ref for diff.
        #[arg(long, default_value = "HEAD~1")]
        base: String,
        /// Head ref for diff.
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Emit raw JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ReleaseCommands {
    /// Show the latest release attempts and canary state.
    Status {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long, default_value = "5")]
        limit: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Continuously refresh the latest release status.
    Watch {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long, default_value = "5")]
        limit: usize,
        #[arg(long, default_value = "5")]
        interval_secs: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Reconcile release attempts against the latest successful upstream pipeline.
    Reconcile {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Trigger approved A/B production promotion for a passed canary.
    PromoteProd {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Check SSH, Vault, registry, and disk before launching canary.
    Preflight {
        #[arg(long)]
        ssh_host: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Diagnose what is blocking canary or production for a release version.
    Doctor {
        #[arg(long)]
        version: Option<String>,
        /// Also run live preflight checks (SSH/Vault/registry/disk).
        #[arg(long, default_value_t = true)]
        preflight: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

pub fn infer_repo_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "jeryu".to_string())
}

#[derive(Subcommand)]
pub(crate) enum SecretsCommands {
    /// Bootstrap and initialize the jeryu-managed Vault.
    Init,
    /// Show Vault health and the latest tracked secret rotation state.
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Rotate release-scoped secrets and render release envs.
    Rotate {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Finalize a previously rotated secret set after promotion succeeds.
    Finalize {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Regenerate the release handoff report from current artifacts.
    Report {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
    },
    /// Print recovery instructions for a release bundle.
    Recover {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum HostCommands {
    /// Perform a storage audit on the host.
    StorageAudit,
    /// Check host, GitLab, Docker, and runner-cache health.
    Doctor {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run an aggressive reclaim operation.
    Reclaim {
        #[arg(long)]
        mode: String,
        #[arg(long, default_value_t = false)]
        plan: bool,
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    /// Install the jeryu-gc systemd timer from ops/ci.
    InstallGcTimer {
        #[arg(long, default_value_t = false)]
        allow_sudo: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum RepoCommands {
    /// Generate the machine-readable agent routing index for jeryu.
    RenderAgentIndex {
        #[arg(long, default_value_t = false)]
        check: bool,
    },
    /// Audit agent-facing routing, docs, and generated index freshness.
    AuditAgentSurface {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run the Postgres-backed state proof in a disposable container.
    PostgresStateProof,
    /// Capture the canonical TUI screenshots used in docs.
    CaptureTuiScreenshots {
        #[arg(long, value_parser = parse_expanded_path)]
        output_dir: Option<PathBuf>,
    },
}
