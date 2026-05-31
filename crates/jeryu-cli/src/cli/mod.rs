//! Pure clap data: the `jeryu` operator/agent command taxonomy.
//!
//! No business logic lives here. Every leaf maps to a [`crate::client::ForgeClient`]
//! method in the dispatch layer. The vocabulary is GitHub-shaped: `pr`,
//! `ci run`, `runner`, and `proof`.

use clap::{Parser, Subcommand};

mod ci;
mod forge;
mod operator;
mod proof;
mod runner;

pub use ci::{CiCommands, CiKindArg};
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
