//! Owner: CLI Runtime Commands
//! Proof: `cargo check -p jeryu`
//! Invariants: All types are pub(crate); main.rs is the only consumer
//!
//! Pure data: clap enum definitions for the non-test `jeryu` CLI commands.

use clap::Subcommand;
use std::path::PathBuf;

use super::{infer_repo_name, parse_expanded_path};

#[derive(Subcommand)]
pub(crate) enum PoolCommands {
    /// List all pools and their managers.
    List,
    /// Scale a pool to N managers.
    Scale { name: String, count: usize },
    /// Pause a pool (stop accepting new jobs).
    Pause { name: String },
    /// Resume a paused pool.
    Resume { name: String },
    /// Drain a pool: pause, wait for jobs to finish, stop managers.
    Drain { name: String },
    /// Drain and remove a pool plus its GitLab runner registration.
    #[clap(name = "delete")]
    Remove { name: String },
    /// Rotate the auth token for a pool.
    RotateToken { name: String },
}

#[derive(Subcommand)]
pub(crate) enum JobCommands {
    /// List jobs for a project.
    List {
        project_id: i64,
        #[arg(long, default_value = "running,pending")]
        status: String,
    },
    /// Show job trace (log output).
    Trace { project_id: i64, job_id: i64 },
    /// Trigger a manual job.
    Play { project_id: i64, job_id: i64 },
    /// Cancel a running job.
    Cancel { project_id: i64, job_id: i64 },
    /// Retry a failed job.
    Retry { project_id: i64, job_id: i64 },
    /// Explain the latest structured failure evidence for a job.
    Explain { project_id: i64, job_id: i64 },
    /// Clear all job and pipeline histories from the database.
    Clear,
}

#[derive(Subcommand)]
pub(crate) enum PipelineCommands {
    /// Explain blocking vs non-blocking state for a specific pipeline.
    Explain {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Diagnose active jobs, runner assignment, and outdated trace symptoms.
    Doctor {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List all jobs with start/end/runtime fields and optionally ingest them.
    Jobs {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        ingest: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Persist all current GitLab job timings for a pipeline.
    Ingest {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
    },
    /// Cancel a superseded or unwanted pipeline.
    Cancel {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
    },
    /// Show historical slow CI jobs from the local jeryu timing ledger.
    Bottlenecks {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref")]
        ref_name: Option<String>,
        #[arg(long, default_value = "25")]
        limit: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum CacheCommands {
    /// Enable and configure Docker daemon for SmartCache registry mirror.
    Enable,
    /// Health-check proxy and registry reachability.
    Doctor,
    /// Show live SmartCache state and metrics.
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run garbage collection on the cache store.
    Gc {
        /// Preview actions without deleting anything.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Emit machine-readable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Preserve cache directories for running runner managers.
        /// Pass --keep-active-managers=false to evict active caches at emergency disk pressure.
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true, default_missing_value = "true", num_args = 0..=1)]
        keep_active_managers: bool,
        /// Only remove orphan manager caches older than this age, e.g. 12h or 2d.
        #[arg(long)]
        older_than: Option<String>,
        /// If total manager cache exceeds this budget, include all orphan caches as candidates.
        #[arg(long)]
        max_cache_gb: Option<f64>,
    },
}

#[derive(Subcommand)]
pub(crate) enum LocalCommands {
    /// Run cargo with jeryu-managed cache roots for a repository checkout.
    Cargo {
        /// Repository root to run cargo in.
        #[arg(long)]
        repo: PathBuf,
        /// Cargo arguments to forward after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        cargo_args: Vec<String>,
    },
    /// Print the cache-aware Cargo environment for a repository checkout.
    CargoEnv {
        /// Repository root to inspect.
        #[arg(long)]
        repo: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum AgentCommands {
    /// Spawn an autonomous agent on a project.
    Spawn {
        project_id: i64,
        /// Description of the task for the agent.
        #[arg(short, long)]
        task: String,
    },
    /// List active agents.
    List { project_id: i64 },
    /// Merge an MR only if the risk gate allows it.
    Merge {
        project_id: i64,
        mr_iid: i64,
        #[arg(long, default_value = "trusted")]
        trust_tier: String,
    },
    /// Agent-first GitHub PR submission: run proof, write evidence capsule, open draft PR.
    Submit {
        /// Short description of the work (used in branch slug + PR title).
        #[arg(short, long)]
        task: String,
        /// Linked issue number (#N). Optional but strongly recommended.
        #[arg(long)]
        issue: Option<u64>,
        /// Risk tier (0..=4). If omitted, inferred from touched paths via release.policy.toml.
        #[arg(long)]
        risk_tier: Option<u8>,
        /// Skip actually opening the PR; only produce the evidence capsule. Default false.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Emit the assembled capsule as JSON to stdout.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum SettingsCommands {
    /// Validate settings and repair a corrupt file backup if present.
    Repair,
    /// Reset `~/.jeryu/settings.json` to defaults.
    Reset {
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[path = "cli_runtime_commands_ext.rs"]
mod cli_runtime_commands_ext;
pub(crate) use cli_runtime_commands_ext::*;
