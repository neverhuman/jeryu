//! Pure clap data: the `jeryu` operator/agent command taxonomy.
//!
//! No business logic lives here. Every leaf maps to a [`crate::client::ForgeClient`]
//! method in the dispatch layer. The vocabulary is GitHub-shaped: `pr`,
//! `ci run`, `runner`, and `proof`.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod agent;
mod ci;
mod control_plane;
mod forge;
mod operator;
mod proof;
mod runner;

pub use agent::{
    AgentAuthCommands, AgentCommands, AgentControlArgs, AgentExportPrArgs, AgentRunArgs,
    AgentToolArg,
};
pub use ci::{CiCommands, CiKindArg};
pub use control_plane::{
    ArtifactsCommands, RepoGraphCommands, RunnersCommands, ToolFinderCommands,
};
pub use forge::{ForgeCommands, IssueCommands, PrCommands, RepoCommands};
pub use operator::{AutonomyCommands, AutonomyInitArgs, AutonomyProfile, GhSetupArgs, OnboardArgs};
pub use proof::ProofCommands;
pub use runner::{RunnerCommands, RunnerExecutorArg};

/// The `jeryu` operator/agent CLI.
#[derive(Debug, Parser)]
#[command(
    name = "jeryu",
    about = "jeryu operator and agent CLI for the jeryu forge",
    long_about = "Operate and automate a jeryu forge: repositories, pull requests, \
issues, CI runs, runners, proofs, releases, and cache.",
    version
)]
pub struct Cli {
    /// Acting owner login for owner-scoped commands.
    #[arg(long, global = true, default_value = "jeryu")]
    pub owner: String,

    /// Emit machine-readable JSON instead of human text.
    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,

    /// Live Jeryu API base URL for agent commands. Defaults to JERYU_API_URL.
    #[arg(long, global = true)]
    pub api_url: Option<String>,

    /// The command to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level command taxonomy.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Forge surfaces: repositories, pull requests, and issues.
    #[command(subcommand)]
    Forge(ForgeCommands),

    /// CI: compile a workflow to IR, schedule a run, inspect, and explain.
    #[command(subcommand)]
    Ci(CiCommands),

    /// Runners: list, enroll, drain, and rotate build runners.
    #[command(subcommand)]
    Runner(RunnerCommands),

    /// Agent-edit: auth, run, control, follow, and PR export.
    #[command(subcommand)]
    Agent(AgentCommands),

    /// Proofs: verify a changeset and explain a blocker.
    #[command(subcommand)]
    Proof(ProofCommands),

    /// Release: compose the signed release-ready gate for a version.
    Release {
        /// Version label to gate (e.g. 3.0.1-rc.1).
        #[arg(long)]
        version: String,
    },

    /// Cache: integrity and content-addressed store operations.
    #[command(subcommand)]
    Cache(CacheCommands),

    /// Current JMCP/control-plane snapshot.
    Status,

    /// Ranked control-plane priorities.
    Priorities {
        /// Maximum number of priorities to fetch.
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Repo graph intelligence and clusters.
    #[command(name = "repo-graph", subcommand)]
    RepoGraph(RepoGraphCommands),

    /// Tool-finder: cross-repo tool candidates and the reusable-tool registry.
    #[command(name = "tool-finder", subcommand)]
    ToolFinder(ToolFinderCommands),

    /// Artifact evidence status.
    #[command(subcommand)]
    Artifacts(ArtifactsCommands),

    /// Runner fabric status.
    #[command(subcommand)]
    Runners(RunnersCommands),

    /// Serve the local forge API and embedded web app.
    Serve {
        /// Address to bind.
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: SocketAddr,

        /// Web asset directory. Release builds embed/pin this artifact; dev can override it.
        #[arg(long, default_value = "apps/web/dist")]
        spa_dir: PathBuf,

        /// Durable Jeryu data directory.
        #[arg(long, default_value = "~/.local/share/jeryu")]
        data_dir: PathBuf,

        /// Split-family manifest used to classify portal and member repositories.
        ///
        /// Repeat this flag to load more than one split family.
        #[arg(long, value_name = "PATH")]
        split_manifest: Vec<PathBuf>,
    },

    /// gh-setup: point the GitHub CLI at a jeryu server base URL.
    #[command(name = "gh-setup")]
    GhSetup(GhSetupArgs),

    /// Autonomy: lay down the canonical autonomy policy bundle.
    #[command(subcommand)]
    Autonomy(AutonomyCommands),

    /// Onboard: rehearse onboarding an existing checkout onto a jeryu forge.
    Onboard(OnboardArgs),
}

/// Cache command group.
#[derive(Debug, Subcommand)]
pub enum CacheCommands {
    /// Run the cache integrity/false-hit self-test and report.
    #[command(name = "self-test")]
    SelfTest,
}
