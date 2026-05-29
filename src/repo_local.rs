//! Owner: Repo-local sidecar worker
//! Proof: `cargo test -p jeryu --lib repo_local`
//! Invariants: Repo sidecars are config-driven, non-blocking for local GitLab merges, and secret-free in repo policy.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::state::Db;

#[path = "repo_local_config.rs"]
mod repo_local_config;
use repo_local_config::*;

#[path = "repo_local_git.rs"]
mod repo_local_git;
use repo_local_git::*;

#[cfg(test)]
#[path = "repo_local_tests.rs"]
mod tests;

#[derive(Debug, Clone, Deserialize)]
pub struct LocalRepoConfig {
    #[serde(skip)]
    pub source_path: PathBuf,
    pub repo: String,
    pub default_branch: String,
    #[serde(default)]
    pub shadow_main: ShadowMainConfig,
    #[serde(default)]
    pub backup: BackupConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShadowMainConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub remote_url: String,
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BackupConfig {
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct SidecarRun {
    pub repo: String,
    pub status: String,
    pub detail: String,
}

pub async fn shadow_main_command(repo: Option<String>) -> Result<i32> {
    let runs = shadow_main_runs(repo).await?;
    for run in runs {
        println!("{} shadow_main: {} {}", run.repo, run.status, run.detail);
    }
    Ok(0)
}

pub async fn backup_command(repo: Option<String>) -> Result<i32> {
    let db = Db::open().await?;
    let configs = matching_configs(repo.as_deref())?;
    if configs.is_empty() {
        bail!("no local repo config matched");
    }

    for config in configs {
        let run = run_backup(&db, &config, "manual").await;
        println!("{} backup: {} {}", run.repo, run.status, run.detail);
    }
    Ok(0)
}

pub async fn shadow_main_runs(repo: Option<String>) -> Result<Vec<SidecarRun>> {
    let db = Db::open().await?;
    let configs = matching_configs(repo.as_deref())?;
    if configs.is_empty() {
        bail!("no local repo config matched");
    }

    let mut runs = Vec::new();
    for config in configs {
        runs.push(run_shadow_main(&db, &config, "manual").await);
    }
    Ok(runs)
}

pub fn open_github_draft_pr(title: &str, body_path: &Path) -> Result<()> {
    let out = Command::new("gh")
        .args([
            "pr",
            "create",
            "--draft",
            "--title",
            title,
            "--body-file",
            &body_path.to_string_lossy(),
        ])
        .output()
        .context("invoke gh pr create (is `gh` installed and authenticated?)")?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "gh pr create failed (exit={:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

pub async fn shadow_main_for_push(db: &Db, repo: Option<&str>, ref_name: &str, after_sha: &str) {
    if is_zero_sha(after_sha) {
        return;
    }

    let Ok(configs) = matching_configs(repo) else {
        tracing::warn!("repo-local shadow config load failed");
        return;
    };

    for config in configs {
        if shadow_ref_matches(&config, ref_name) {
            let _ = run_shadow_main(db, &config, "webhook").await;
        }
    }
}

pub async fn reconcile_repo_sidecars(db: &Db) -> Result<Vec<SidecarRun>> {
    let mut runs = Vec::new();
    for config in load_configs()? {
        if config.shadow_main.enabled {
            runs.push(run_shadow_main(db, &config, "reconcile").await);
        }
        if !config.backup.target.trim().is_empty() {
            runs.push(run_backup(db, &config, "reconcile").await);
        }
    }
    Ok(runs)
}
