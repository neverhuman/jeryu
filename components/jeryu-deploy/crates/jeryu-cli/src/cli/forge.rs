//! `jeryu forge` taxonomy: repositories, pull requests, and issues.
//!
//! Pull requests are GitHub-shaped: a pull request is opened from a `--head`
//! branch into a `--base` branch and is addressed by a per-repo `#number`.

use clap::Subcommand;

/// Forge command group.
#[derive(Debug, Subcommand)]
pub enum ForgeCommands {
    /// Repository operations.
    #[command(subcommand)]
    Repo(RepoCommands),

    /// Pull request operations.
    #[command(subcommand)]
    Pr(PrCommands),

    /// Issue operations.
    #[command(subcommand)]
    Issue(IssueCommands),
}

/// `jeryu forge repo` operations.
#[derive(Debug, Subcommand)]
pub enum RepoCommands {
    /// Create a repository under the acting owner.
    Create {
        /// Repository name.
        name: String,
        /// Create the repository as private.
        #[arg(long, default_value_t = false)]
        private: bool,
        /// Default branch for the new repository.
        #[arg(long, default_value = "main")]
        default_branch: String,
    },
    /// List repositories for the acting owner.
    List,
}

/// `jeryu forge pr` operations.
#[derive(Debug, Subcommand)]
pub enum PrCommands {
    /// Open a pull request from a head branch into a base branch.
    Open {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
        /// Source branch carrying the changes.
        #[arg(long)]
        head: String,
        /// Target branch to merge into.
        #[arg(long, default_value = "main")]
        base: String,
        /// Pull request title.
        #[arg(long)]
        title: String,
        /// Open the pull request as a draft.
        #[arg(long, default_value_t = false)]
        draft: bool,
    },
    /// List pull requests for a repository.
    List {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
    },
    /// Show the status of a pull request by number.
    Status {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
        /// Pull request number (#N).
        #[arg(long)]
        pr: u64,
    },
    /// Merge a pull request, subject to the risk gate.
    Merge {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
        /// Pull request number (#N).
        #[arg(long)]
        pr: u64,
        /// Trust tier for the risk gate.
        #[arg(long, default_value = "trusted")]
        trust_tier: String,
    },
}

/// `jeryu forge issue` operations.
#[derive(Debug, Subcommand)]
pub enum IssueCommands {
    /// Create an issue in a repository.
    Create {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
        /// Issue title.
        #[arg(long)]
        title: String,
        /// Optional issue body.
        #[arg(long)]
        body: Option<String>,
    },
    /// List issues in a repository.
    List {
        /// Repository name (under the acting owner).
        #[arg(long)]
        repo: String,
    },
}
