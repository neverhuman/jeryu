//! Operator and onboarding taxonomy: `jeryu gh-setup`, `jeryu autonomy`, and
//! `jeryu onboard`.
//!
//! These verbs are the day-zero surface an operator runs *before* the forge is
//! fully wired: point the GitHub CLI at a jeryu server, lay down the canonical
//! autonomy policy bundle, and rehearse the onboarding plan for an existing
//! checkout. They are local/printable (no live server transport yet) so they
//! are deterministic and snapshot-testable.

use clap::{Args, Subcommand, ValueEnum};

/// `jeryu gh-setup`: point the GitHub CLI at a jeryu server base URL.
///
/// jeryu serves `GET /user` and `GET /api/v3/user`, so a `gh` host entry
/// pointed at the jeryu base URL makes status checks resolve against jeryu.
/// If auth looks wrong, rerun this command; do not start a `gh auth login` or
/// refresh flow against the Jeryu host. Idempotent: re-running with the same
/// host/token source reproduces the same entry.
#[derive(Debug, Args)]
pub struct GhSetupArgs {
    /// jeryu server base URL the GitHub CLI should target.
    #[arg(long, default_value = "http://localhost:8080")]
    pub host: String,

    /// OAuth token to record for the host. Overrides --token-file when both are provided.
    #[arg(long)]
    pub token: Option<String>,

    /// Path to an OAuth token file (defaults to ~/.jeryu/secrets/merge-token).
    #[arg(long, value_name = "PATH")]
    pub token_file: Option<String>,

    /// Print the resulting config to stdout instead of writing the hosts file.
    #[arg(long, default_value_t = false)]
    pub print: bool,

    /// Override the hosts file path (defaults to the GitHub CLI default).
    #[arg(long)]
    pub path: Option<String>,
}

/// `jeryu autonomy`: manage the on-disk autonomy policy bundle.
#[derive(Debug, Subcommand)]
pub enum AutonomyCommands {
    /// Emit the canonical `.jeryu/autonomy/policies/*.yml` bundle plus the
    /// `.jeryu/ci.toml` and `.jeryu/policy.toml` control files.
    Init(AutonomyInitArgs),
}

/// Autonomy profile: how far up the risk ladder auto-merge is allowed.
///
/// Every profile keeps the safety floor intact: R5 stays fail-closed and the
/// protected-paths hard-human floor is never relaxed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AutonomyProfile {
    /// Canonical baseline: R0-R2 auto-merge, R3-R5 human-required.
    Baseline,
    /// Aggressive: R0-R4 auto-merge, R5 fail-closed (safety floor intact).
    #[value(name = "full-auto")]
    FullAuto,
}

/// Arguments for `jeryu autonomy init`.
#[derive(Debug, Args)]
pub struct AutonomyInitArgs {
    /// Autonomy profile to encode.
    #[arg(long, value_enum, default_value_t = AutonomyProfile::FullAuto)]
    pub profile: AutonomyProfile,

    /// Directory to write the `.jeryu/` tree under (defaults to the cwd).
    #[arg(long, default_value = ".")]
    pub path: String,

    /// Print the bundle to stdout instead of writing it to disk.
    #[arg(long, default_value_t = false)]
    pub print: bool,
}

/// `jeryu onboard`: rehearse onboarding an existing checkout onto a jeryu forge.
///
/// Prints the ordered plan (create + materialize on the server, repoint the
/// `origin` remote, register, set the autonomy profile). Dry-run only for now:
/// the live server transport is not wired, so this never mutates the checkout.
#[derive(Debug, Args)]
pub struct OnboardArgs {
    /// Path to the existing repository checkout to onboard.
    pub path: String,

    /// jeryu server base URL the checkout will be repointed at.
    #[arg(long, default_value = "http://localhost:8080")]
    pub host: String,

    /// Owner login the repository is created under on the server.
    #[arg(long, default_value = "jeryu")]
    pub owner: String,

    /// Print the plan without executing it. Required: server transport is not
    /// yet live, so the plan is currently dry-run only.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}
