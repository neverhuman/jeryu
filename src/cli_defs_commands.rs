use clap::Subcommand;
use std::path::PathBuf;

#[path = "cli_defs_commands_bug.rs"]
mod cli_defs_commands_bug;
pub(crate) use cli_defs_commands_bug::BugAttemptCommands;

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
pub(crate) enum AccessCommands {
    /// Diagnose local GitLab access for a checkout or the known workspace set.
    Doctor {
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        all_known: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Repair local GitLab access for a checkout or the known workspace set.
    Repair {
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        all_known: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Resolve a GitLab project by path or by local repo checkout.
    Project {
        project: Option<String>,
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Inspect the machine-readable remote-key metadata.
    #[command(subcommand)]
    Keys(AccessKeyCommands),
}

#[derive(Subcommand)]
pub(crate) enum AccessKeyCommands {
    /// List the configured remote-key metadata.
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Doctor the configured remote-key metadata.
    Doctor {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum MrCommands {
    /// Create a merge request for the current local GitLab checkout.
    Create {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value_t = false)]
        draft: bool,
        #[arg(long, default_value_t = false)]
        push: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[path = "cli_defs_commands_repo.rs"]
mod cli_defs_commands_repo;
pub(crate) use cli_defs_commands_repo::{
    RepoCommands, RepoFleetCommands, RepoHookCommands, RepoStandardCommand, RepoStandardCommands,
};
