//! Owner: Multi-repo fleet registry and health collector
//! Proof: `cargo test -p jeryu --lib repo_fleet`
//! Invariants: Registry parsing is deterministic; GitHub and local git health normalize into one snapshot.

use crate::git_host::{GitHubClient, RepoRef};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_REGISTRY_PATH: &str = ".jeryu/repos.toml";
pub const DEFAULT_REPO_SLUG: &str = "neverhuman/jeryu";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RepoRegistry {
    #[serde(default)]
    pub schema_version: Option<String>,
    #[serde(default)]
    pub repo: Vec<RepoConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RepoConfig {
    pub alias: String,
    pub slug: String,
    pub provider: String,
    pub remote: String,
    pub local_root: PathBuf,
    pub default_branch: String,
    pub visibility: String,
    pub health_profile: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RepoLocalStatus {
    pub exists: bool,
    pub branch: Option<String>,
    pub sha_short: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct RepoRunSummary {
    pub run_id: Option<i64>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoActivityEvent {
    pub repo_slug: String,
    pub alias: String,
    pub event_kind: String,
    pub status: String,
    pub title: String,
    pub url: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FleetRepoSnapshot {
    pub alias: String,
    pub slug: String,
    pub provider: String,
    pub default_branch: String,
    pub visibility: String,
    pub health_profile: String,
    pub status: String,
    pub running_count: u32,
    pub failed_count: u32,
    pub stale: bool,
    pub score_badge: Option<String>,
    pub local: RepoLocalStatus,
    pub latest_run: Option<RepoRunSummary>,
    pub next_command: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct FleetSnapshot {
    pub generated_at: String,
    pub registry_path: String,
    pub repos: Vec<FleetRepoSnapshot>,
    pub events: Vec<RepoActivityEvent>,
}

impl FleetSnapshot {
    pub fn selected(&self, selected_index: usize) -> Option<&FleetRepoSnapshot> {
        if selected_index == 0 {
            return None;
        }
        self.repos.get(selected_index.saturating_sub(1))
    }

    pub fn counts(&self) -> (u32, u32, u32) {
        let running = self.repos.iter().map(|repo| repo.running_count).sum();
        let failed = self.repos.iter().map(|repo| repo.failed_count).sum();
        let aged = self.repos.iter().filter(|repo| repo.stale).count() as u32;
        (running, failed, aged)
    }
}

pub fn load_registry_from(repo_root: &Path) -> Result<RepoRegistry> {
    let path = repo_root.join(DEFAULT_REGISTRY_PATH);
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read fleet registry {}", path.display()))?;
    let registry: RepoRegistry =
        toml::from_str(&raw).with_context(|| format!("parse fleet registry {}", path.display()))?;
    Ok(registry)
}

pub fn load_registry_path(path: &Path) -> Result<RepoRegistry> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read fleet registry {}", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("parse fleet registry {}", path.display()))
}

pub fn registry_path_for(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_REGISTRY_PATH)
}

pub fn local_git_status(repo: &RepoConfig) -> RepoLocalStatus {
    if !repo.local_root.is_dir() {
        return RepoLocalStatus::default();
    }
    let branch = git_output(&repo.local_root, &["branch", "--show-current"]);
    let sha_short = git_output(&repo.local_root, &["rev-parse", "--short", "HEAD"]);
    let dirty = git_output(&repo.local_root, &["status", "--porcelain"])
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false);
    RepoLocalStatus {
        exists: true,
        branch,
        sha_short,
        dirty,
    }
}

pub async fn collect_fleet_snapshot(
    repo_root: &Path,
    github: Option<&GitHubClient>,
) -> Result<FleetSnapshot> {
    let registry = load_registry_from(repo_root)?;
    collect_fleet_snapshot_from_registry(&registry, registry_path_for(repo_root), github).await
}

pub async fn collect_fleet_snapshot_from_registry(
    registry: &RepoRegistry,
    registry_path: PathBuf,
    github: Option<&GitHubClient>,
) -> Result<FleetSnapshot> {
    let generated_at = chrono::Utc::now().to_rfc3339();
    let mut repos = Vec::new();
    let mut events = Vec::new();

    for repo in &registry.repo {
        let local = local_git_status(repo);
        let score_badge = load_score_badge(&repo.local_root);
        let mut latest_run = None;
        let mut running_count = 0;
        let mut failed_count = 0;
        let mut stale = false;

        if repo.provider == "github"
            && let Some(client) = github
            && let Some(repo_ref) = RepoRef::parse(&repo.slug)
            && let Ok(runs) = client
                .list_workflow_runs(&repo_ref, Some(&repo.default_branch), 5)
                .await
        {
            for run in &runs {
                if is_running_status(run.status.as_deref()) {
                    running_count += 1;
                }
                if is_failed_conclusion(run.conclusion.as_deref()) {
                    failed_count += 1;
                }
                events.push(RepoActivityEvent {
                    repo_slug: repo.slug.clone(),
                    alias: repo.alias.clone(),
                    event_kind: "workflow_run".into(),
                    status: run
                        .conclusion
                        .clone()
                        .or_else(|| run.status.clone())
                        .unwrap_or_else(|| "unknown".into()),
                    title: run.name.clone().unwrap_or_else(|| "workflow run".into()),
                    url: run.html_url.clone(),
                    observed_at: run
                        .updated_at
                        .clone()
                        .unwrap_or_else(|| generated_at.clone()),
                });
            }
            latest_run = runs.first().map(|run| RepoRunSummary {
                run_id: run.id,
                name: run.name.clone(),
                status: run.status.clone(),
                conclusion: run.conclusion.clone(),
                html_url: run.html_url.clone(),
                updated_at: run.updated_at.clone(),
            });
            stale = latest_run
                .as_ref()
                .and_then(|run| run.updated_at.as_deref())
                .and_then(parse_utc)
                .map(|updated| {
                    chrono::Utc::now()
                        .signed_duration_since(updated)
                        .num_hours()
                        > 24
                })
                .unwrap_or(false);
        }

        let status = classify_repo_status(&local, latest_run.as_ref(), running_count, failed_count);
        repos.push(FleetRepoSnapshot {
            alias: repo.alias.clone(),
            slug: repo.slug.clone(),
            provider: repo.provider.clone(),
            default_branch: repo.default_branch.clone(),
            visibility: repo.visibility.clone(),
            health_profile: repo.health_profile.clone(),
            status,
            running_count,
            failed_count,
            stale,
            score_badge,
            local,
            latest_run,
            next_command: format!("cd {} && just fast", repo.local_root.display()),
        });
    }

    Ok(FleetSnapshot {
        generated_at,
        registry_path: registry_path.display().to_string(),
        repos,
        events,
    })
}

pub fn print_registry_list(registry: &RepoRegistry) {
    println!(
        "{:<10} {:<34} {:<8} {:<16} local_root",
        "alias", "slug", "provider", "profile"
    );
    for repo in &registry.repo {
        println!(
            "{:<10} {:<34} {:<8} {:<16} {}",
            repo.alias,
            repo.slug,
            repo.provider,
            repo.health_profile,
            repo.local_root.display()
        );
    }
}

pub fn print_fleet_status(snapshot: &FleetSnapshot) {
    println!(
        "{:<10} {:<10} {:<7} {:<7} {:<6} {:<8} command",
        "alias", "status", "running", "failed", "aged", "score"
    );
    for repo in &snapshot.repos {
        println!(
            "{:<10} {:<10} {:<7} {:<7} {:<6} {:<8} {}",
            repo.alias,
            repo.status,
            repo.running_count,
            repo.failed_count,
            repo.stale,
            repo.score_badge.as_deref().unwrap_or("-"),
            repo.next_command
        );
    }
}

fn classify_repo_status(
    local: &RepoLocalStatus,
    latest_run: Option<&RepoRunSummary>,
    running_count: u32,
    failed_count: u32,
) -> String {
    if !local.exists {
        return "missing".into();
    }
    if local.dirty {
        return "dirty".into();
    }
    if failed_count > 0 {
        return "failed".into();
    }
    if running_count > 0 {
        return "running".into();
    }
    match latest_run.and_then(|run| run.conclusion.as_deref()) {
        Some("success") => "green".into(),
        Some("failure" | "cancelled" | "timed_out" | "action_required") => "failed".into(),
        Some(_) => "unknown".into(),
        None => "local".into(),
    }
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_string())
}

fn load_score_badge(root: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(root.join("target/jankurai/repo-score.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let score = value
        .get("score")
        .or_else(|| value.pointer("/summary/score"))
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|v| v.round() as i64)))?;
    Some(format!("{score}"))
}

fn is_running_status(status: Option<&str>) -> bool {
    matches!(
        status,
        Some("queued" | "in_progress" | "requested" | "waiting" | "pending")
    )
}

fn is_failed_conclusion(conclusion: Option<&str>) -> bool {
    matches!(
        conclusion,
        Some("failure" | "cancelled" | "timed_out" | "action_required")
    )
}

fn parse_utc(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_registry_profiles() {
        let raw = r#"
schema_version = "1"

[[repo]]
alias = "nht"
slug = "neverhuman/veox-nht"
provider = "github"
remote = "https://github.com/neverhuman/veox-nht.git"
local_root = "/tmp/veox-nht"
default_branch = "main"
visibility = "private"
health_profile = "rust-workspace"
"#;
        let registry: RepoRegistry = toml::from_str(raw).unwrap();
        assert_eq!(registry.repo.len(), 1);
        assert_eq!(registry.repo[0].alias, "nht");
        assert_eq!(registry.repo[0].health_profile, "rust-workspace");
    }

    #[test]
    fn classifies_local_dirty_before_remote_state() {
        let local = RepoLocalStatus {
            exists: true,
            dirty: true,
            ..RepoLocalStatus::default()
        };
        assert_eq!(classify_repo_status(&local, None, 0, 0), "dirty");
    }
}
