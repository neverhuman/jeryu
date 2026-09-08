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

/// jeryu-tool-finder + reusable-tool registry command group.
#[derive(Debug, Subcommand)]
pub enum ToolFinderCommands {
    /// Show cross-repo repeated-code clusters — candidates for shared tools.
    Clusters {
        /// Repo id to query; defaults to the whole-family cross-repo scan.
        #[arg(long, default_value = "family/jeryu-split")]
        repo: String,
        /// Maximum number of clusters to fetch.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Show the reusable-tool registry summary (the golden-box numbers).
    Summary,
}
