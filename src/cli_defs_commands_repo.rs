use clap::{Args, Subcommand};
use std::path::PathBuf;

use jeryu::repo::{HookMode, HookProfile, RepoMode};
use jeryu::repo_standard::StandardProvider;

use crate::cli::{infer_repo_name, parse_expanded_path};

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
    /// Read-only git/auth hygiene audit (HTTP-PAT origin, stale AGENTS.md, missing .jeryu).
    AuditHygiene {
        /// Audit a specific checkout (defaults to the current directory).
        #[arg(long, value_parser = parse_expanded_path)]
        path: Option<PathBuf>,
        /// Sweep every repo in the fleet registry instead of a single checkout.
        #[arg(long, default_value_t = false)]
        fleet: bool,
        /// Optional explicit fleet registry path (with --fleet).
        #[arg(long, value_parser = parse_expanded_path)]
        registry: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Configure the repo-managed git hook directory for this checkout.
    InstallGitHooks,
    /// Initialize a new local checkout backed by local JeRyu/GitLab.
    Init(RepoInitCommand),
    /// Adopt an existing checkout into direct local JeRyu/GitLab use.
    Adopt(RepoAdoptCommand),
    /// Switch local JeRyu repository mode.
    Mode {
        #[arg(value_enum)]
        mode: RepoMode,
    },
    /// Manage optional local hook profiles.
    #[command(subcommand)]
    Hooks(RepoHookCommands),
    /// Plan, apply, or verify the canonical agent-first repo standard.
    #[command(subcommand)]
    Standard(RepoStandardCommands),
    /// List, status, or sync registered multi-repo fleet entries.
    #[command(subcommand)]
    Fleet(RepoFleetCommands),
    /// Push configured main refs to server-owned shadow remotes.
    Shadow {
        #[arg(long)]
        repo: Option<String>,
    },
    /// Sync configured bare mirrors and repo-local sidecar config to backups.
    Backup {
        #[arg(long)]
        repo: Option<String>,
    },
    /// Run the fast changed-file Jankurai guard manually.
    JankuraiFast {
        #[arg(long, default_value = "origin/main")]
        changed_from: String,
    },
    /// Run the RedlineDB-backed state proof against embedded file state.
    RedlineStateProof,
    /// Capture the canonical TUI screenshots used in docs.
    CaptureTuiScreenshots {
        #[arg(long, value_parser = parse_expanded_path)]
        output_dir: Option<PathBuf>,
    },
}

#[derive(Args)]
pub(crate) struct RepoInitCommand {
    pub name: String,
    #[arg(long, default_value_t = false)]
    pub direct: bool,
    #[arg(long, default_value = "root")]
    pub namespace: String,
    #[arg(long, default_value = "main")]
    pub branch: String,
    #[arg(long, default_value_t = true)]
    pub protect_main: bool,
    #[arg(long, value_enum, default_value_t = HookMode::Off)]
    pub hooks: HookMode,
    /// Allow JeRyu, and only JeRyu, to relay approved updates to protected main.
    #[arg(long, default_value_t = false)]
    pub main_relay: bool,
    /// Offline release mirror URL for bundles/tags. Credentials must live outside .jeryu.
    #[arg(long)]
    pub offline_release_remote: Option<String>,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Args)]
pub(crate) struct RepoAdoptCommand {
    #[arg(default_value = ".", value_parser = parse_expanded_path)]
    pub path: PathBuf,
    #[arg(long, default_value_t = false)]
    pub direct: bool,
    #[arg(long, default_value = "root")]
    pub namespace: String,
    #[arg(long, default_value_t = infer_repo_name())]
    pub name: String,
    #[arg(long, default_value_t = true)]
    pub protect_main: bool,
    #[arg(long, value_enum, default_value_t = HookMode::Off)]
    pub hooks: HookMode,
    /// Allow JeRyu, and only JeRyu, to relay approved updates to protected main.
    #[arg(long, default_value_t = false)]
    pub main_relay: bool,
    /// Offline release mirror URL for bundles/tags. Credentials must live outside .jeryu.
    #[arg(long)]
    pub offline_release_remote: Option<String>,
    #[arg(long, default_value_t = false)]
    pub replace_origin: bool,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Subcommand)]
pub(crate) enum RepoStandardCommands {
    /// Print the managed-file changes needed to match the standard.
    Plan(RepoStandardCommand),
    /// Write the managed standard files and configure local hooks.
    Apply(RepoStandardCommand),
    /// Fail if the checkout has drifted from the managed standard.
    Verify(RepoStandardCommand),
}

#[derive(Args, Clone)]
pub(crate) struct RepoStandardCommand {
    #[arg(default_value = ".", value_parser = parse_expanded_path)]
    pub path: PathBuf,
    #[arg(long, default_value = "sovereign_plus")]
    pub profile: String,
    #[arg(long, value_enum, default_value_t = StandardProvider::Github)]
    pub provider: StandardProvider,
    #[arg(long, default_value = "main")]
    pub base_branch: String,
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long, default_value = ".jeryu/autonomy")]
    pub autonomy_dir: PathBuf,
    #[arg(long, default_value_t = true)]
    pub configure_git_hooks: bool,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Subcommand)]
pub(crate) enum RepoFleetCommands {
    /// List registered repositories from the fleet registry.
    List(RepoFleetCommand),
    /// Print local and optional GitHub status for registered repositories.
    Status(RepoFleetStatusCommand),
    /// Persist registered repositories into the Jeryu state store.
    Sync(RepoFleetCommand),
}

#[derive(Args, Clone)]
pub(crate) struct RepoFleetCommand {
    #[arg(long, value_parser = parse_expanded_path)]
    pub registry: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Args, Clone)]
pub(crate) struct RepoFleetStatusCommand {
    #[arg(long, value_parser = parse_expanded_path)]
    pub registry: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, default_value_t = false)]
    pub github: bool,
}

#[derive(Subcommand)]
pub(crate) enum RepoHookCommands {
    Status,
    Enable {
        #[arg(long, value_enum, default_value_t = HookMode::Advisory)]
        mode: HookMode,
    },
    Disable,
    Install {
        #[arg(long, value_enum, default_value_t = HookProfile::PrePush)]
        profile: HookProfile,
        #[arg(long, value_enum, default_value_t = HookMode::Advisory)]
        mode: HookMode,
    },
}
