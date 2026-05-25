//! Owner: CLI Test Commands
//! Proof: `cargo check -p jeryu`
//! Invariants: All types are pub(crate); main.rs is the only consumer
//!
//! Pure data: clap enum definitions for the `jeryu test` subtree.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum TestCommands {
    /// Run a single test command through a CI pipeline.
    Run {
        /// The test command to execute.
        #[arg(short, long)]
        command: String,
        #[arg(long)]
        project_id: Option<i64>,
        #[arg(long, default_value = "rust:1.92.0")]
        image: String,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
        #[arg(long)]
        force: bool,
    },
    /// Preview the inferred runner class and timeout for a command.
    Plan {
        #[arg(short, long)]
        command: String,
        #[arg(long)]
        project_id: Option<i64>,
        #[arg(long, default_value = "rust:1.92.0")]
        image: String,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
    },
    /// Run multiple test commands in parallel through separate pipelines.
    Batch {
        #[arg(short = 'c', long = "command", required = true)]
        commands: Vec<String>,
        #[arg(long)]
        project_id: Option<i64>,
        #[arg(long, default_value = "rust:1.92.0")]
        image: String,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long, default_value = "600")]
        timeout: u64,
        #[arg(long, default_value = "3")]
        max_parallel: usize,
        #[arg(long)]
        force: bool,
    },

    /// Show results of all jobs in a pipeline.
    Results {
        pipeline_id: i64,
        #[arg(long)]
        project_id: Option<i64>,
    },
    /// Requeue a specific failed job by name.
    Requeue {
        pipeline_id: i64,
        job_name: String,
        #[arg(long)]
        project_id: Option<i64>,
    },
    /// Show only failed jobs from a pipeline with their traces.
    Failed {
        pipeline_id: i64,
        #[arg(long)]
        project_id: Option<i64>,
    },
    /// Ask the checked-out project which CI jobs and release gates a diff needs.
    Impact {
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Smart test selection: compute the minimal test plan for a diff.
    #[clap(name = "select")]
    Choose {
        /// Base ref (e.g. origin/main, HEAD~1, SHA).
        #[arg(long, default_value = "origin/main")]
        base: String,
        /// Head ref (e.g. HEAD, SHA).
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Repository root to resolve changed files.
        #[arg(long)]
        repo_root: Option<PathBuf>,
        /// Print the plan explanation.
        #[arg(long, default_value_t = false)]
        explain: bool,
        /// Emit raw JSON plan.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Write the generated GitLab child pipeline YAML to this path.
        #[arg(long)]
        emit_gitlab: Option<PathBuf>,
        /// Write the JSON plan to this path.
        #[arg(long)]
        emit_plan: Option<PathBuf>,
        /// Write the VTI proof receipt JSON to this path.
        #[arg(long)]
        emit_receipt: Option<PathBuf>,
    },
    /// Explain a test plan (from JSON file or the last computed plan).
    ExplainPlan {
        /// Path to a JSON test plan file.
        plan_path: PathBuf,
    },
    /// Smart test selection for an external workspace with a `.jeryu/testmap.toml`.
    SelectExternal {
        /// Base ref (e.g. origin/main, HEAD~1, SHA).
        #[arg(long, default_value = "origin/main")]
        base: String,
        /// Head ref (e.g. HEAD, SHA).
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Path to the external workspace root (must contain .jeryu/testmap.toml).
        #[arg(long)]
        workspace: PathBuf,
        /// Print the plan explanation.
        #[arg(long, default_value_t = false)]
        explain: bool,
        /// Emit raw JSON plan.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Write the generated GitLab child pipeline YAML to this path.
        #[arg(long)]
        emit_gitlab: Option<PathBuf>,
        /// Write the JSON plan to this path.
        #[arg(long)]
        emit_plan: Option<PathBuf>,
        /// Write JSON metadata for jobs omitted by VTI to this path.
        #[arg(long)]
        emit_skipped: Option<PathBuf>,
    },
    /// Audit VTI accuracy: compare full test results against what VTI would have selected.
    Audit {
        /// Comma-separated list of changed paths.
        #[arg(long)]
        changed: String,
        /// Comma-separated list of failed test names.
        #[arg(long, default_value = "")]
        failed: String,
        /// Comma-separated list of all test names.
        #[arg(long)]
        all_tests: String,
        /// The SHA this audit covers.
        #[arg(long, default_value = "HEAD")]
        sha: String,
        /// Emit JSON output.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// The optional workspace path if running externally.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Learn from an audit report and suggest rule improvements.
    Learn {
        /// Comma-separated list of changed paths.
        #[arg(long)]
        changed: String,
        /// Comma-separated list of failed test names.
        #[arg(long, default_value = "")]
        failed: String,
        /// Comma-separated list of all test names.
        #[arg(long)]
        all_tests: String,
        /// The SHA this learning covers.
        #[arg(long, default_value = "HEAD")]
        sha: String,
        /// Emit JSON output.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// The optional workspace path if running externally.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Show cache status for test commands against the current source state.
    CacheStatus {
        /// Base ref for diff.
        #[arg(long, default_value = "HEAD~1")]
        base: String,
        /// Head ref for diff.
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Emit raw JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}
