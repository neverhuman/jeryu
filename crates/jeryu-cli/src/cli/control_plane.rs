//! JMCP/control-plane command taxonomy.

use clap::Subcommand;

/// Repo graph command group.
#[derive(Debug, Subcommand)]
pub enum RepoGraphCommands {
    /// Show graph clusters.
    Clusters {
        /// Restrict clusters to a single kind.
        #[arg(long)]
        cluster_kind: Option<String>,
        /// Maximum number of graph nodes/clusters/insights to fetch.
        #[arg(long)]
        limit: Option<usize>,
    },
}

/// Artifact evidence command group.
#[derive(Debug, Subcommand)]
pub enum ArtifactsCommands {
    /// Show latest build/release artifact evidence.
    Latest {
        /// Optional repo slug for future scoped evidence.
        #[arg(long)]
        repo: Option<String>,
    },
}

/// Runner fabric command group.
#[derive(Debug, Subcommand)]
pub enum RunnersCommands {
    /// Show local runner fabric plus optional mirror evidence.
    Status,
}
