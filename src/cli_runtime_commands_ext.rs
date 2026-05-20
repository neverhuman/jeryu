use clap::Subcommand;
use std::path::PathBuf;

use super::{infer_repo_name, parse_expanded_path};

#[derive(Subcommand)]
pub(crate) enum ReleaseCommands {
    /// Show the latest release attempts and canary state.
    Status {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long, default_value = "5")]
        limit: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Continuously refresh the latest release status.
    Watch {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long, default_value = "5")]
        limit: usize,
        #[arg(long, default_value = "5")]
        interval_secs: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Reconcile release attempts, resuming the current release by default.
    Reconcile {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        /// Force a fresh upstream pipeline selection instead of resuming the current release.
        #[arg(long, default_value_t = false)]
        fresh: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Trigger approved A/B production promotion for a passed canary.
    PromoteProd {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Check SSH, Vault, registry, and disk before launching canary.
    Preflight {
        #[arg(long)]
        ssh_host: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Diagnose what is blocking canary or production for a release version.
    Doctor {
        #[arg(long)]
        version: Option<String>,
        /// Also run live preflight checks (SSH/Vault/registry/disk).
        #[arg(long, default_value_t = true)]
        preflight: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Compose the jeryu/release-ready gate for a PR and optionally emit a GitHub Check Run.
    Ready {
        /// PR number (GitHub). 0 in local rehearsals.
        #[arg(long, default_value_t = 0)]
        pr: u64,
        /// Emit a GitHub Check Run via `gh api` (requires gh + GITHUB_TOKEN).
        #[arg(long, default_value_t = false)]
        emit_status: bool,
        /// Do not call gh; print the assembled gate to stdout only.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run the full release pipeline locally except the final publish step.
    DryRun {
        /// Version under preparation (e.g. 3.0.1-rc.1).
        #[arg(long)]
        version: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Tag + push + trigger release.yml. Requires a recent successful dry-run.
    Submit {
        #[arg(long)]
        version: String,
        /// Skip the freshness check on the cached dry-run result (NOT recommended).
        #[arg(long, default_value_t = false)]
        force: bool,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Mark an agent PR as human-approved after CI is green. Refuses self-approval.
    Approve {
        #[arg(long)]
        pr: u64,
        /// Override the approver identity for testing. Production runs read from gh CLI.
        #[arg(long)]
        as_user: Option<String>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Walk the declared rollback path for a released version. Never re-tags.
    Rollback {
        #[arg(long)]
        version: String,
        /// Reason for rollback (free text, recorded in rollback.json).
        #[arg(long)]
        reason: String,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum SecretsCommands {
    /// Bootstrap and initialize the jeryu-managed Vault.
    Init,
    /// Show Vault health and the latest tracked secret rotation state.
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Rotate release-scoped secrets and render release envs.
    Rotate {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Finalize a previously rotated secret set after promotion succeeds.
    Finalize {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Regenerate the release handoff report from current artifacts.
    Report {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
    },
    /// Print recovery instructions for a release bundle.
    Recover {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum HostCommands {
    /// Perform a storage audit on the host.
    StorageAudit,
    /// Check host, GitLab, Docker, and runner-cache health.
    Doctor {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run an aggressive reclaim operation.
    Reclaim {
        #[arg(long)]
        mode: String,
        #[arg(long, default_value_t = false)]
        plan: bool,
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    /// Install the jeryu-gc systemd timer from ops/ci.
    ///
    /// Use this for manual re-install only. `jeryu bootstrap` now installs
    /// the always-on `jeryu-gcd.service` plus this timer as a deep-sweep
    /// safety net; prefer bootstrap for initial setup.
    InstallGcTimer {
        #[arg(long, default_value_t = false)]
        allow_sudo: bool,
    },
    /// Install the always-on `jeryu-gcd.service` (disk-pressure daemon).
    ///
    /// Maintains df ≥ 80 GiB free via pressure-tier GC. Auto-invoked by
    /// `jeryu bootstrap`; this command is for manual re-install or
    /// recovery.
    InstallGcdService {
        #[arg(long, default_value_t = false)]
        allow_sudo: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum RepoCommands {
    /// Generate the machine-readable agent routing index for jeryu.
    RenderAgentIndex {
        #[arg(long, default_value_t = false)]
        check: bool,
    },
    /// Audit agent-facing routing, docs, and generated index freshness.
    AuditAgentSurface {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Configure the repo-managed git hook directory for this checkout.
    InstallGitHooks,
    /// Run the RedlineDB-backed state proof against embedded file state.
    RedlineStateProof,
    /// Capture the canonical TUI screenshots used in docs.
    CaptureTuiScreenshots {
        #[arg(long, value_parser = parse_expanded_path)]
        output_dir: Option<PathBuf>,
    },
}
