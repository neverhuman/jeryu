//! CLI definitions for `jeryu node` — SSH remote Docker node management.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum NodeCommands {
    /// Register a remote SSH node for runner-manager deployment.
    ///
    /// Probes the node for Docker availability, records its config, and
    /// makes it available for pool scale-up immediately.
    ///
    /// Example:
    ///   jeryu node add deploy@xbabe0
    ///   jeryu node add deploy@xbabe0 --alias xbabe0 --gitlab-url http://192.168.1.2:8929
    Add {
        /// SSH target in the form user@host or host.
        target: String,
        /// Short alias used in logs, TUI, and config file names (defaults to hostname part of target).
        #[arg(long)]
        alias: Option<String>,
        /// SSH port (default 22).
        #[arg(long, default_value_t = 22)]
        ssh_port: u16,
        /// Path to SSH identity file.  Omit to use ssh-agent or default key.
        #[arg(long)]
        identity: Option<String>,
        /// Maximum number of runner-manager containers to run on this node.
        #[arg(long, default_value_t = 4)]
        max_managers: usize,
        /// Total storage budget in GiB.  Auto-GC triggers at 90%.
        #[arg(long, default_value_t = 100.0)]
        storage_limit_gb: f64,
        /// Restrict this node to specific pool names (comma-separated).
        /// Omit to allow this node to serve any pool.
        #[arg(long, value_delimiter = ',')]
        pool_affinity: Vec<String>,
        /// Override the GitLab URL that runner containers use to reach GitLab.
        /// Required when the control-plane default (http://gitlab.local:<port>) is
        /// not reachable from the remote node.
        #[arg(long)]
        gitlab_url: Option<String>,
        /// Print what would happen without making any changes.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// List all registered remote nodes.
    List {
        /// Output as JSON for scripting/agent consumption.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Remove a registered remote node.
    ///
    /// Stops all runner-manager containers on the node first unless --force.
    Remove {
        /// Node alias to remove.
        alias: String,
        /// Remove without stopping running managers.
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Check connectivity and health of a registered node.
    Doctor {
        /// Node alias to probe.
        alias: String,
    },
}
