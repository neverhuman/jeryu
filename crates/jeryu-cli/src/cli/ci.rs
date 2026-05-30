//! `jeryu ci` taxonomy: compile a workflow to IR, schedule a run, inspect, and
//! explain.
//!
//! CI is a compile -> IR -> schedule model. `ci run` takes a workflow *file*
//! and a *ref*, never a remote pipeline id to poll. The only accepted input
//! dialects are GitHub Actions YAML and the native jeryu TOML; there is no
//! foreign-CI emit path.

use clap::{Subcommand, ValueEnum};

use crate::client::CiKind;

/// CI workflow input dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CiKindArg {
    /// GitHub Actions workflow YAML.
    Github,
    /// Native jeryu TOML pipeline.
    Native,
}

impl From<CiKindArg> for CiKind {
    fn from(value: CiKindArg) -> Self {
        match value {
            CiKindArg::Github => CiKind::GithubActions,
            CiKindArg::Native => CiKind::NativeToml,
        }
    }
}

/// CI command group.
#[derive(Debug, Subcommand)]
pub enum CiCommands {
    /// Compile a workflow file to IR and schedule a run for a ref.
    Run {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
        /// Git ref to compile against.
        #[arg(long = "ref", default_value = "main")]
        git_ref: String,
        /// Workflow input dialect.
        #[arg(long, value_enum, default_value_t = CiKindArg::Native)]
        kind: CiKindArg,
    },
    /// Report queued and scheduled runs for a repository.
    Status {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
    },
    /// Explain whether a run is blocked from merging and why.
    Explain {
        /// Run identifier to explain.
        run_id: String,
    },
}
