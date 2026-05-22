//! Owner: Repo standardizer
//! Proof: `cargo test -p jeryu --lib repo_standard`
//! Invariants: Agent-first shipping standards render deterministically and keep repo-owned policy under `.jeryu/`.

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::Serialize;
use std::fmt;
use std::path::PathBuf;

#[path = "repo_standard_render.rs"]
mod repo_standard_render;
use repo_standard_render::render_standard_files;

#[path = "repo_standard_git.rs"]
mod repo_standard_git;
use repo_standard_git::{
    git_config_get, infer_remote_slug, normalize_relative_path, run_git, set_executable,
    sha256_hex, split_repo_slug,
};

#[path = "repo_standard_plan.rs"]
mod plan;
use plan::{apply_standard, plan_standard, print_report, report_is_clean};

#[cfg(test)]
#[path = "repo_standard_tests.rs"]
mod tests;

pub const AGENT_FIRST_STANDARD_VERSION: &str = "agent-first-autonomous-v1";
const DEFAULT_AUTONOMY_DIR: &str = ".jeryu/autonomy";
const REQUIRED_CHECK_NAME: &str = "jeryu/required";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum StandardProvider {
    Github,
    Gitlab,
}

impl StandardProvider {
    fn label(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
        }
    }
}

impl fmt::Display for StandardProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoStandardMode {
    Plan,
    Apply,
    Verify,
}

#[derive(Debug, Clone)]
pub struct RepoStandardOptions {
    pub path: PathBuf,
    pub profile: String,
    pub provider: StandardProvider,
    pub base_branch: String,
    pub repo_slug: Option<String>,
    pub autonomy_dir: PathBuf,
    pub configure_git_hooks: bool,
    pub json: bool,
}

#[derive(Debug, Serialize)]
pub struct RepoStandardReport {
    pub status: RepoStandardStatus,
    pub repo_path: String,
    pub standard_version: String,
    pub profile: String,
    pub provider: StandardProvider,
    pub base_branch: String,
    pub repo_slug: String,
    pub autonomy_dir: String,
    pub required_check: String,
    pub changes: Vec<ManagedFileChange>,
    pub hook_config: HookConfigChange,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepoStandardStatus {
    Clean,
    Planned,
    Applied,
    Drift,
}

#[derive(Debug, Serialize)]
pub struct ManagedFileChange {
    pub path: String,
    pub operation: ManagedFileOperation,
    pub executable: bool,
    pub sha256: String,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedFileOperation {
    Create,
    Update,
    Unchanged,
}

#[derive(Debug, Serialize)]
pub struct HookConfigChange {
    pub desired: String,
    pub actual: Option<String>,
    pub operation: ManagedFileOperation,
}

#[derive(Debug, Clone)]
pub(crate) struct StandardSpec {
    repo_root: PathBuf,
    profile: String,
    provider: StandardProvider,
    base_branch: String,
    repo_slug: String,
    repo_owner: String,
    repo_name: String,
    autonomy_dir: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedFile {
    path: &'static str,
    content: String,
    executable: bool,
}

pub fn run_standard(mode: RepoStandardMode, opts: RepoStandardOptions) -> Result<i32> {
    let spec = build_spec(&opts)?;
    let files = render_standard_files(&spec);
    let mut report = plan_standard(&spec, &files, &opts)?;

    match mode {
        RepoStandardMode::Plan => {
            report.status = if report_is_clean(&report) {
                RepoStandardStatus::Clean
            } else {
                RepoStandardStatus::Planned
            };
            print_report(&report, opts.json)?;
            Ok(0)
        }
        RepoStandardMode::Apply => {
            apply_standard(&spec.repo_root, &files)?;
            if opts.configure_git_hooks && spec.repo_root.join(".git").is_dir() {
                run_git(
                    &spec.repo_root,
                    &["config", "--local", "core.hooksPath", ".jeryu/hooks"],
                )?;
            }
            report = plan_standard(&spec, &files, &opts)?;
            report.status = RepoStandardStatus::Applied;
            print_report(&report, opts.json)?;
            Ok(0)
        }
        RepoStandardMode::Verify => {
            report.status = if report_is_clean(&report) {
                RepoStandardStatus::Clean
            } else {
                RepoStandardStatus::Drift
            };
            let clean = report.status == RepoStandardStatus::Clean;
            print_report(&report, opts.json)?;
            Ok(if clean { 0 } else { 1 })
        }
    }
}

fn build_spec(opts: &RepoStandardOptions) -> Result<StandardSpec> {
    let repo_root = opts
        .path
        .canonicalize()
        .with_context(|| format!("resolving {}", opts.path.display()))?;
    if !repo_root.is_dir() {
        bail!("{} is not a directory", repo_root.display());
    }
    if repo_root.join(".autonomy").exists() {
        bail!(
            "root .autonomy is forbidden by {AGENT_FIRST_STANDARD_VERSION}; move policy under {DEFAULT_AUTONOMY_DIR} and remove .autonomy before standardizing"
        );
    }

    let repo_slug = opts
        .repo_slug
        .clone()
        .or_else(|| infer_remote_slug(&repo_root).ok().flatten())
        .unwrap_or_else(|| "unknown/unknown".to_string());
    let (repo_owner, repo_name) = split_repo_slug(&repo_slug);
    let autonomy_dir = normalize_relative_path(&opts.autonomy_dir)?;
    if !autonomy_dir.starts_with(".jeryu/") && autonomy_dir != ".jeryu" {
        bail!("autonomy-dir must live under .jeryu/; got {autonomy_dir}");
    }
    if autonomy_dir != DEFAULT_AUTONOMY_DIR {
        bail!("autonomy-dir currently must be {DEFAULT_AUTONOMY_DIR}; got {autonomy_dir}");
    }

    Ok(StandardSpec {
        repo_root,
        profile: opts.profile.clone(),
        provider: opts.provider,
        base_branch: opts.base_branch.clone(),
        repo_slug,
        repo_owner,
        repo_name,
        autonomy_dir,
    })
}
