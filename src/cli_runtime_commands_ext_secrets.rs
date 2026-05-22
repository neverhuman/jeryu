use clap::Subcommand;
use std::path::PathBuf;

use crate::cli::{infer_repo_name, parse_expanded_path};

#[derive(Subcommand)]
pub(crate) enum SecretsCommands {
    /// Bootstrap and initialize the jeryu-managed Vault.
    #[command(alias = "init")]
    Provision {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show Vault health and the latest tracked secret rotation state.
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run read-only Vault diagnostics and fail if the Vault is not healthy.
    Doctor {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Rotate release-scoped secrets and render release envs.
    Rotate {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long, value_parser = parse_expanded_path)]
        repo_root: Option<PathBuf>,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Finalize a previously rotated secret set after promotion succeeds.
    Finalize {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long, value_parser = parse_expanded_path)]
        repo_root: Option<PathBuf>,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Regenerate the release handoff report from current artifacts.
    Report {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long, value_parser = parse_expanded_path)]
        repo_root: Option<PathBuf>,
        #[arg(long)]
        version: String,
    },
    /// Print recovery instructions for a release bundle.
    Recover {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long, value_parser = parse_expanded_path)]
        repo_root: Option<PathBuf>,
        #[arg(long)]
        version: String,
    },
}
