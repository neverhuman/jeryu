//! Owner: Multi-repo fleet registry and health collector
//! Proof: `cargo test -p jeryu --lib repo_fleet`
//! Invariants: Registry parsing is deterministic; GitHub and local git health normalize into one snapshot.

use crate::git_host::{GitHubClient, RepoRef};
use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_REGISTRY_PATH: &str = ".jeryu/repos.toml";
pub const DEFAULT_REPO_SLUG: &str = "neverhuman/jeryu";
const LOCAL_WORKSPACE_ROOT_DEFAULT: &str = "/home/ubuntu/veox-repos";
/// Namespace schema version used to derive per-repo cache and data namespaces.
pub const FLEET_NAMESPACE_SCHEMA_VERSION: &str = "v1";

// ---------------------------------------------------------------------------
// Scope model — Phase 0 (fleet drill-down)
// ---------------------------------------------------------------------------

/// Discriminant for a [`FleetScopeItem`] in the scope navigator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetScopeKind {
    /// The global "ALL" entry that shows the entire fleet.
    All,
    /// A family group (e.g. `veox-*`) that aggregates repos sharing a common
    /// dash-prefix alias.
    Family,
    /// A single repository.
    Repo,
}

/// Aggregate health metrics for a group of repos in the scope navigator.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FleetScopeRollup {
    pub repo_count: usize,
    pub running_count: u32,
    pub failed_count: u32,
    pub aged_count: u32,
    /// RFC 3339 timestamp of the most-recent activity in this scope.
    pub latest_activity_at: Option<String>,
    /// 0–100 utilisation pressure derived from running jobs per repo.
    pub utilization_pressure: u16,
}

/// One chip in the fleet-bar scope navigator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetScopeItem {
    pub kind: FleetScopeKind,
    /// Display label: `"ALL"`, `"veox-*"`, or a repo alias.
    pub label: String,
    /// Populated for `Family` and `Repo` items; `None` for `All`.
    pub family: Option<String>,
    /// Index into `FleetSnapshot::repos` for `Repo` items; `None` otherwise.
    pub repo_index: Option<usize>,
    pub rollup: FleetScopeRollup,
    /// Derived health status: `"running"` | `"failed"` | `"aged"` | `"green"`.
    pub status: String,
    /// Cache namespace for this repo (populated for `Repo` items only).
    pub cache_namespace: Option<String>,
    /// Data namespace for this repo (populated for `Repo` items only).
    pub data_namespace: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RepoRegistry {
    #[serde(default)]
    pub schema_version: Option<String>,
    #[serde(default)]
    pub repo: Vec<RepoConfig>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoConfig {
    pub alias: String,
    pub slug: String,
    pub provider: String,
    pub remote: String,
    pub local_root: PathBuf,
    pub default_branch: String,
    pub visibility: String,
    pub health_profile: String,
    /// Explicit family override; auto-derived from alias dash-prefix when `None`.
    pub family: Option<String>,
}

impl<'de> Deserialize<'de> for RepoConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawRepoConfig {
            alias: String,
            slug: String,
            #[serde(default)]
            provider: Option<String>,
            remote: String,
            #[serde(default)]
            local_root: Option<PathBuf>,
            #[serde(default)]
            path: Option<PathBuf>,
            default_branch: String,
            #[serde(default)]
            visibility: Option<String>,
            #[serde(default)]
            health_profile: Option<String>,
            #[serde(default)]
            profile: Option<String>,
            #[serde(default)]
            family: Option<String>,
        }

        let raw = RawRepoConfig::deserialize(deserializer)?;
        let local_root = raw
            .local_root
            .or(raw.path)
            .ok_or_else(|| serde::de::Error::missing_field("local_root"))?;
        Ok(Self {
            alias: raw.alias,
            provider: raw
                .provider
                .unwrap_or_else(|| infer_provider(&raw.slug, &raw.remote).to_string()),
            slug: raw.slug,
            remote: raw.remote,
            local_root,
            default_branch: raw.default_branch,
            visibility: raw.visibility.unwrap_or_else(|| "private".to_string()),
            health_profile: raw
                .health_profile
                .or(raw.profile)
                .unwrap_or_else(|| "default".to_string()),
            family: raw.family,
        })
    }
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
    // Scope-navigator fields (Phase 0)
    /// Family group this repo belongs to (auto-derived or explicit).
    pub family: Option<String>,
    /// RFC 3339 timestamp of the most-recent activity for this repo.
    pub last_activity_at: Option<String>,
    /// Derived: `jeryu-cache-v1-<slug_safe>` where `/` → `__`.
    pub cache_namespace: String,
    /// Derived: `jeryu-data-v1-<slug_safe>` where `/` → `__`.
    pub data_namespace: String,
    /// 0–100 utilisation pressure (running jobs × 5, clamped).
    pub utilization_pressure: u16,
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

    // -----------------------------------------------------------------------
    // Scope navigator (Phase 0)
    // -----------------------------------------------------------------------

    /// Build the chip list for the fleet-bar scope navigator.
    ///
    /// - `family_drill = None`  → root level: ALL + family groups + standalone repos
    /// - `family_drill = Some("veox")` → drilled: family aggregate + child repos
    pub fn scope_items(&self, family_drill: Option<&str>) -> Vec<FleetScopeItem> {
        match family_drill {
            None => self.top_level_scopes(),
            Some(family) => self.drilldown_scopes(family),
        }
    }

    /// Root-level chips: ALL + one chip per family group + one chip per
    /// standalone repo (alias with no family).
    pub fn top_level_scopes(&self) -> Vec<FleetScopeItem> {
        let all_indices: Vec<usize> = (0..self.repos.len()).collect();
        let all_rollup = self.rollup_for_indices(&all_indices);
        let all_status = Self::scope_status(&all_rollup);
        let mut items = vec![FleetScopeItem {
            kind: FleetScopeKind::All,
            label: "ALL".into(),
            family: None,
            repo_index: None,
            rollup: all_rollup,
            status: all_status,
            cache_namespace: None,
            data_namespace: None,
        }];

        // Group repos by family (BTreeMap for deterministic alpha order)
        let mut families: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
        let mut standalone: Vec<usize> = Vec::new();
        for (i, repo) in self.repos.iter().enumerate() {
            match &repo.family {
                Some(f) => families.entry(f.clone()).or_default().push(i),
                None => standalone.push(i),
            }
        }

        for (family, indices) in &families {
            let rollup = self.rollup_for_indices(indices);
            let status = Self::scope_status(&rollup);
            items.push(FleetScopeItem {
                kind: FleetScopeKind::Family,
                label: format!("{family}-*"),
                family: Some(family.clone()),
                repo_index: None,
                rollup,
                status,
                cache_namespace: None,
                data_namespace: None,
            });
        }

        for i in standalone {
            let repo = &self.repos[i];
            let rollup = self.rollup_for_indices(&[i]);
            let status = Self::scope_status(&rollup);
            items.push(FleetScopeItem {
                kind: FleetScopeKind::Repo,
                label: repo.alias.clone(),
                family: repo.family.clone(),
                repo_index: Some(i),
                rollup,
                status,
                cache_namespace: Some(repo.cache_namespace.clone()),
                data_namespace: Some(repo.data_namespace.clone()),
            });
        }

        items
    }

    /// Drilled chips for a specific family: family-aggregate first, then child repos.
    pub fn drilldown_scopes(&self, family: &str) -> Vec<FleetScopeItem> {
        let child_indices: Vec<usize> = self
            .repos
            .iter()
            .enumerate()
            .filter(|(_, r)| r.family.as_deref() == Some(family))
            .map(|(i, _)| i)
            .collect();

        if child_indices.is_empty() {
            return Vec::new();
        }

        let rollup = self.rollup_for_indices(&child_indices);
        let status = Self::scope_status(&rollup);
        let mut items = vec![FleetScopeItem {
            kind: FleetScopeKind::Family,
            label: format!("{family}-*"),
            family: Some(family.to_string()),
            repo_index: None,
            rollup,
            status,
            cache_namespace: None,
            data_namespace: None,
        }];

        for i in child_indices {
            let repo = &self.repos[i];
            let rollup = self.rollup_for_indices(&[i]);
            let status = Self::scope_status(&rollup);
            items.push(FleetScopeItem {
                kind: FleetScopeKind::Repo,
                label: repo.alias.clone(),
                family: repo.family.clone(),
                repo_index: Some(i),
                rollup,
                status,
                cache_namespace: Some(repo.cache_namespace.clone()),
                data_namespace: Some(repo.data_namespace.clone()),
            });
        }

        items
    }

    /// Aggregate health metrics across a slice of repo indices.
    pub fn rollup_for_indices(&self, indices: &[usize]) -> FleetScopeRollup {
        let mut running_count: u32 = 0;
        let mut failed_count: u32 = 0;
        let mut aged_count: u32 = 0;
        let mut latest_activity_at: Option<String> = None;
        let mut total_pressure: u32 = 0;

        for &i in indices {
            if let Some(repo) = self.repos.get(i) {
                running_count += repo.running_count;
                failed_count += repo.failed_count;
                if repo.stale {
                    aged_count += 1;
                }
                total_pressure += repo.utilization_pressure as u32;
                if let Some(ref at) = repo.last_activity_at {
                    match &latest_activity_at {
                        None => latest_activity_at = Some(at.clone()),
                        Some(existing) if at > existing => {
                            latest_activity_at = Some(at.clone());
                        }
                        _ => {}
                    }
                }
            }
        }

        let utilization_pressure = if indices.is_empty() {
            0
        } else {
            ((total_pressure / indices.len() as u32) as u16).min(100)
        };

        FleetScopeRollup {
            repo_count: indices.len(),
            running_count,
            failed_count,
            aged_count,
            latest_activity_at,
            utilization_pressure,
        }
    }

    fn scope_status(rollup: &FleetScopeRollup) -> String {
        if rollup.failed_count > 0 {
            return "failed".into();
        }
        if rollup.running_count > 0 {
            return "running".into();
        }
        if rollup.aged_count > 0 {
            return "aged".into();
        }
        "green".into()
    }
}

/// The active repo scope of the TUI. Derived from `App::selected_repo_index`
/// via `App::repo_filter()`. Default of `All` is the multi-repo overview;
/// `Family { family }` filters to all repos in a dash-prefix group;
/// `Only { alias, slug }` filters every pane to a single repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoFilter<'a> {
    All,
    /// All repos whose alias starts with `{family}-`.
    Family {
        family: &'a str,
    },
    Only {
        alias: &'a str,
        slug: &'a str,
    },
}

impl<'a> RepoFilter<'a> {
    pub fn is_all(&self) -> bool {
        matches!(self, RepoFilter::All)
    }

    /// Returns a display label for the active scope.
    pub fn label(&self) -> &'a str {
        match self {
            RepoFilter::All => "All",
            RepoFilter::Family { family } => family,
            RepoFilter::Only { alias, .. } => alias,
        }
    }

    /// Test whether an item with the given (optional) repo identifiers
    /// passes the current filter. An item carrying `None` for both is
    /// shown only under `All` — unfiltered items should not leak into a
    /// per-repo view.
    pub fn matches(&self, item_alias: Option<&str>, item_slug: Option<&str>) -> bool {
        match self {
            RepoFilter::All => true,
            RepoFilter::Family { family } => match item_alias {
                None => false,
                Some(a) => a.starts_with(&format!("{family}-")) || a == *family,
            },
            RepoFilter::Only { alias, slug } => {
                item_alias == Some(alias) || item_slug == Some(slug)
            }
        }
    }

    /// Convert to an owned form that does not borrow from any source. Used
    /// by callers that need to hold the filter across a mutable borrow of
    /// the same data structure that produced it.
    pub fn to_owned(self) -> OwnedRepoFilter {
        match self {
            RepoFilter::All => OwnedRepoFilter::All,
            RepoFilter::Family { family } => OwnedRepoFilter::Family {
                family: family.to_string(),
            },
            RepoFilter::Only { alias, slug } => OwnedRepoFilter::Only {
                alias: alias.to_string(),
                slug: slug.to_string(),
            },
        }
    }
}

/// Owned counterpart of [`RepoFilter`]. Use when you need the filter to
/// outlive the borrow of the snapshot that produced it (typically in
/// methods that take `&mut self`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedRepoFilter {
    All,
    /// Owned counterpart of [`RepoFilter::Family`].
    Family {
        family: String,
    },
    Only {
        alias: String,
        slug: String,
    },
}

impl OwnedRepoFilter {
    pub fn as_ref(&self) -> RepoFilter<'_> {
        match self {
            OwnedRepoFilter::All => RepoFilter::All,
            OwnedRepoFilter::Family { family } => RepoFilter::Family {
                family: family.as_str(),
            },
            OwnedRepoFilter::Only { alias, slug } => RepoFilter::Only {
                alias: alias.as_str(),
                slug: slug.as_str(),
            },
        }
    }

    pub fn is_all(&self) -> bool {
        matches!(self, OwnedRepoFilter::All)
    }

    pub fn matches(&self, item_alias: Option<&str>, item_slug: Option<&str>) -> bool {
        self.as_ref().matches(item_alias, item_slug)
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

pub fn registry_from_tracked_repositories(
    repos: &[crate::state::TrackedRepository],
) -> RepoRegistry {
    RepoRegistry {
        schema_version: Some("state-store".to_string()),
        repo: repos
            .iter()
            .map(|repo| RepoConfig {
                alias: repo.alias.clone(),
                slug: repo.slug.clone(),
                provider: repo.provider.clone(),
                remote: repo.remote.clone(),
                local_root: PathBuf::from(&repo.local_root),
                default_branch: repo.default_branch.clone(),
                visibility: repo.visibility.clone(),
                health_profile: repo.health_profile.clone(),
                family: None,
            })
            .collect(),
    }
}

pub async fn collect_fleet_snapshot_from_tracked_repositories(
    repos: &[crate::state::TrackedRepository],
    github: Option<&GitHubClient>,
) -> Result<FleetSnapshot> {
    let registry = registry_from_tracked_repositories(repos);
    collect_fleet_snapshot_from_registry(
        &registry,
        PathBuf::from("state:tracked_repositories"),
        github,
    )
    .await
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

/// Ensure the local daemon/TUI has a stable workspace-root default when no
/// explicit override is present. This is only applied when the configured
/// fallback workspace actually exists on disk.
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

    // Pre-compute which alias dash-prefixes appear 2+ times so we can
    // auto-derive family groups without needing explicit `family =` in TOML.
    let family_set = compute_family_set(&registry.repo);

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
        let last_activity_at = latest_run.as_ref().and_then(|r| r.updated_at.clone());
        let slug_safe = repo.slug.replace('/', "__");
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
            family: repo
                .family
                .clone()
                .or_else(|| derive_alias_family(&repo.alias, &family_set)),
            last_activity_at,
            cache_namespace: format!("jeryu-cache-{FLEET_NAMESPACE_SCHEMA_VERSION}-{slug_safe}"),
            data_namespace: format!("jeryu-data-{FLEET_NAMESPACE_SCHEMA_VERSION}-{slug_safe}"),
            utilization_pressure: (running_count * 5).min(100) as u16,
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

fn infer_provider(slug: &str, remote: &str) -> &'static str {
    let remote = remote.to_ascii_lowercase();
    if slug.starts_with("root/") || remote.contains("gitlab") {
        "gitlab"
    } else {
        "github"
    }
}

/// Compute the set of alias dash-prefixes that appear on 2+ repos.
/// Only prefixes where the alias actually contains a dash are counted.
fn compute_family_set(repos: &[RepoConfig]) -> std::collections::HashSet<String> {
    let mut counts: std::collections::HashMap<String, usize> = Default::default();
    for repo in repos {
        if let Some(prefix) = repo.alias.split('-').next()
            && prefix != repo.alias
        {
            *counts.entry(prefix.to_string()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c >= 2)
        .map(|(prefix, _)| prefix)
        .collect()
}

/// Derive the family string for a repo alias given the pre-computed family set.
/// Returns `None` when the alias has no dash or the prefix doesn't form a group.
fn derive_alias_family(
    alias: &str,
    family_set: &std::collections::HashSet<String>,
) -> Option<String> {
    let prefix = alias.split('-').next()?;
    if prefix == alias {
        return None; // no dash
    }
    if family_set.contains(prefix) {
        Some(prefix.to_string())
    } else {
        None
    }
}

fn parse_utc(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Set `JERYU_WORKSPACE_ROOT` to a durable local default if it is not already
/// set. Called once at process startup (from `dispatch::run`) so that the
/// daemon/TUI inherit the local workspace path and child processes see the
/// same value.
pub fn ensure_workspace_root_default() {
    // Already set — nothing to do.
    if std::env::var("JERYU_WORKSPACE_ROOT").is_ok() {
        return;
    }

    let default_root = Path::new(LOCAL_WORKSPACE_ROOT_DEFAULT);
    if default_root.join(DEFAULT_REGISTRY_PATH).exists() {
        // SAFETY: single-threaded startup; no other thread reads this var yet.
        unsafe { std::env::set_var("JERYU_WORKSPACE_ROOT", default_root) };
        return;
    }

    // Upward cwd walk (sync, no subprocess).
    if let Ok(mut dir) = std::env::current_dir() {
        loop {
            if dir.join(DEFAULT_REGISTRY_PATH).exists() {
                // SAFETY: single-threaded startup; no other thread reads this var yet.
                unsafe { std::env::set_var("JERYU_WORKSPACE_ROOT", &dir) };
                return;
            }
            if !dir.pop() {
                break;
            }
        }
    }

    // Daemon /proc scan (Linux only).
    #[cfg(target_os = "linux")]
    if let Some(root) = serve_daemon_workspace_root() {
        // SAFETY: single-threaded startup; no other thread reads this var yet.
        unsafe { std::env::set_var("JERYU_WORKSPACE_ROOT", &root) };
    }
}

/// Find the workspace root that owns a `.jeryu/repos.toml` fleet registry.
///
/// Three strategies are tried in order, and the first match wins:
///
/// 1. `JERYU_WORKSPACE_ROOT` env var — explicit override for daemon / remote contexts.
/// 2. Walk upward from `current_dir()` — works regardless of which subdirectory the
///    TUI was launched from (mirrors how `git` finds `.git/`).
/// 3. `git rev-parse --show-toplevel` — handles detached worktrees, symlinks, and
///    other edge cases where the upward walk might miss.
///
/// Returns `None` when no parent directory with `.jeryu/repos.toml` is found, which
/// is a valid configuration (first-run or non-monorepo workspace).
pub(crate) async fn resolve_workspace_root() -> Option<PathBuf> {
    // Strategy 1: explicit env override
    if let Ok(p) = std::env::var("JERYU_WORKSPACE_ROOT") {
        let p = PathBuf::from(p);
        if p.join(DEFAULT_REGISTRY_PATH).exists() {
            return Some(p);
        }
    }

    // Strategy 2: walk upward from cwd
    if let Ok(mut dir) = std::env::current_dir() {
        loop {
            if dir.join(DEFAULT_REGISTRY_PATH).exists() {
                return Some(dir);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    // Strategy 3: git rev-parse --show-toplevel
    if let Ok(out) = tokio::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .await
        && out.status.success()
    {
        let root = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        if root.join(DEFAULT_REGISTRY_PATH).exists() {
            return Some(root);
        }
    }

    // Strategy 4 (Linux): find a running `jeryu serve` daemon and borrow its cwd.
    // This handles the common case where the user runs `jeryu tui` from an
    // unrelated directory (e.g. $HOME) but has a daemon running from the project root.
    #[cfg(target_os = "linux")]
    if let Some(root) = serve_daemon_workspace_root() {
        return Some(root);
    }

    None
}

/// Scan /proc for a running `jeryu serve` process and return its cwd if it
/// contains the registry file.  Linux-only, best-effort (silently returns None
/// on any I/O error or permission failure).
#[cfg(target_os = "linux")]
fn serve_daemon_workspace_root() -> Option<PathBuf> {
    serve_daemon_workspace_root_from_proc(Path::new("/proc"))
}

#[cfg(target_os = "linux")]
fn serve_daemon_workspace_root_from_proc(proc_root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(proc_root).ok()?;
    for entry in entries.flatten() {
        let pid_dir = entry.path();
        if !pid_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
        {
            continue;
        }
        if !process_is_jeryu_serve(&pid_dir.join("cmdline")) {
            continue;
        }
        if let Some(cwd) = serve_pid_cwd(&pid_dir)
            && cwd.join(DEFAULT_REGISTRY_PATH).exists()
        {
            return Some(cwd);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn process_is_jeryu_serve(cmdline_path: &Path) -> bool {
    let Ok(cmdline) = std::fs::read(cmdline_path) else {
        return false;
    };
    let args: Vec<&[u8]> = cmdline.split(|&b| b == 0).collect();
    let is_jeryu = args
        .first()
        .and_then(|a| std::str::from_utf8(a).ok())
        .map(|a| a.ends_with("jeryu") || a.ends_with("/jeryu"))
        .unwrap_or(false);
    let has_serve = args.iter().skip(1).any(|a| *a == b"serve");
    is_jeryu && has_serve
}

#[cfg(target_os = "linux")]
fn serve_pid_cwd(pid_dir: &Path) -> Option<PathBuf> {
    std::fs::read_link(pid_dir.join("cwd")).ok()
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
    fn parses_legacy_veox_split_registry_defaults() {
        let raw = r#"
schema_version = "1"
workspace = "veox-fusion"

[[repo]]
name = "veox-shared"
alias = "shared"
slug = "neverhuman/veox-shared"
path = "/tmp/veox-shared"
remote = "https://github.com/neverhuman/veox-shared.git"
default_branch = "main"
profile = "rust-workspace"
project_id = "veox-shared"
"#;
        let registry: RepoRegistry = toml::from_str(raw).unwrap();
        assert_eq!(registry.repo.len(), 1);
        assert_eq!(registry.repo[0].alias, "shared");
        assert_eq!(registry.repo[0].provider, "github");
        assert_eq!(registry.repo[0].visibility, "private");
        assert_eq!(registry.repo[0].health_profile, "rust-workspace");
        assert_eq!(
            registry.repo[0].local_root,
            PathBuf::from("/tmp/veox-shared")
        );
    }

    #[test]
    fn tracked_repositories_render_as_fleet_registry() {
        let registry = registry_from_tracked_repositories(&[crate::state::TrackedRepository {
            slug: "neverhuman/veox-shared".into(),
            alias: "shared".into(),
            provider: "github".into(),
            remote: "https://github.com/neverhuman/veox-shared.git".into(),
            local_root: "/tmp/veox-shared".into(),
            default_branch: "main".into(),
            visibility: "private".into(),
            health_profile: "split-required".into(),
            created_at: "2026-05-24T00:00:00Z".into(),
            updated_at: "2026-05-24T00:00:00Z".into(),
        }]);

        assert_eq!(registry.schema_version.as_deref(), Some("state-store"));
        assert_eq!(registry.repo[0].alias, "shared");
        assert_eq!(
            registry.repo[0].local_root,
            PathBuf::from("/tmp/veox-shared")
        );
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

    #[tokio::test]
    async fn resolve_workspace_root_finds_toml_by_walking_up() {
        use tempfile::TempDir;

        // Build: <tmp>/child/grandchild  with  <tmp>/.jeryu/repos.toml
        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".jeryu");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(registry_dir.join("repos.toml"), b"schema_version = \"1\"\n").unwrap();

        let child = tmp.path().join("child").join("grandchild");
        std::fs::create_dir_all(&child).unwrap();

        // Setup under lock, then drop before the await (clippy::await_holding_lock).
        let prev_cwd = {
            let _guard = crate::test_sync::PATH_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let prev_ws = std::env::var("JERYU_WORKSPACE_ROOT").ok();
            // Clear env override so we exercise the upward-walk strategy.
            // SAFETY: serialised by PATH_ENV_LOCK.
            if prev_ws.is_some() {
                unsafe { std::env::remove_var("JERYU_WORKSPACE_ROOT") };
            }
            let prev_cwd = std::env::current_dir().unwrap();
            std::env::set_current_dir(&child).unwrap();
            prev_cwd
            // _guard dropped; env restored in the cleanup block below.
        };

        let result = resolve_workspace_root().await;

        // Restore cwd and env; acquire lock to serialise the removal.
        {
            let _guard = crate::test_sync::PATH_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            std::env::set_current_dir(&prev_cwd).unwrap();
        }

        let found = result.expect("resolve_workspace_root should find the toml via upward walk");
        // Canonicalize both sides so tmp symlinks don't cause spurious mismatches.
        assert_eq!(
            found.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn resolve_workspace_root_respects_env_override() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let registry_dir = tmp.path().join(".jeryu");
        std::fs::create_dir_all(&registry_dir).unwrap();
        std::fs::write(registry_dir.join("repos.toml"), b"schema_version = \"1\"\n").unwrap();

        // Setup under lock, then drop before the await (clippy::await_holding_lock).
        let prev_ws = {
            let _guard = crate::test_sync::PATH_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let prev = std::env::var("JERYU_WORKSPACE_ROOT").ok();
            // SAFETY: serialised by PATH_ENV_LOCK.
            unsafe { std::env::set_var("JERYU_WORKSPACE_ROOT", tmp.path()) };
            prev
            // _guard dropped here.
        };

        let result = resolve_workspace_root().await;

        // Restore env under lock.
        {
            let _guard = crate::test_sync::PATH_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            // SAFETY: serialised by PATH_ENV_LOCK.
            unsafe {
                match prev_ws {
                    Some(v) => std::env::set_var("JERYU_WORKSPACE_ROOT", v),
                    None => std::env::remove_var("JERYU_WORKSPACE_ROOT"),
                }
            }
        }

        let found = result.expect("resolve_workspace_root should honour JERYU_WORKSPACE_ROOT");
        assert_eq!(
            found.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    /// Strategy 4: the serve-daemon /proc scan returns the daemon's cwd when it
    /// contains .jeryu/repos.toml.  We can't spawn a real daemon in a unit test,
    /// but we can verify the helper function `serve_daemon_workspace_root` by
    /// pointing it at *ourselves* — the test process has a known cwd that we
    /// control.  We write the registry into a temp dir, set cwd there, and
    /// confirm the scanner finds it.
    ///
    /// NOTE: we look for *our own* process in /proc by searching for a
    /// `jeryu serve` cmdline — which our test binary obviously doesn't have.
    /// So instead we unit-test the underlying helper directly: we verify that
    /// a synthetic /proc layout (with a mocked cmdline + cwd) is parsed
    /// correctly.  The integration-level property (TUI launched from $HOME
    /// finds repos via the daemon) is validated by the existing tuiwright test.
    #[cfg(target_os = "linux")]
    #[test]
    fn serve_daemon_workspace_root_reads_daemon_cwd() {
        use tempfile::TempDir;

        // Build a fake /proc/<pid>/ directory with a `jeryu serve` cmdline and a
        // cwd symlink pointing to a temp dir that has .jeryu/repos.toml.
        let fake_proc = TempDir::new().unwrap();
        let pid_dir = fake_proc.path().join("12345");
        std::fs::create_dir_all(&pid_dir).unwrap();

        // Write a NUL-delimited cmdline: b"jeryu\0serve\0\0"
        std::fs::write(pid_dir.join("cmdline"), b"jeryu\x00serve\x00\x00").unwrap();

        // Create the workspace in another temp dir
        let workspace = TempDir::new().unwrap();
        std::fs::create_dir_all(workspace.path().join(".jeryu")).unwrap();
        std::fs::write(
            workspace.path().join(".jeryu/repos.toml"),
            b"schema_version = \"1\"\n",
        )
        .unwrap();

        // Create a cwd symlink in the fake /proc pid dir pointing to the workspace
        std::os::unix::fs::symlink(workspace.path(), pid_dir.join("cwd")).unwrap();

        let result = serve_daemon_workspace_root_from_proc(fake_proc.path());

        let found = result.expect("should discover workspace from fake daemon cwd");
        assert_eq!(
            found.canonicalize().unwrap(),
            workspace.path().canonicalize().unwrap()
        );
    }

    // -----------------------------------------------------------------------
    // Scope-model unit tests (Phase 0)
    // -----------------------------------------------------------------------

    fn make_snap(repos: Vec<FleetRepoSnapshot>) -> FleetSnapshot {
        FleetSnapshot {
            generated_at: "2026-01-01T00:00:00Z".into(),
            registry_path: ".jeryu/repos.toml".into(),
            repos,
            events: Vec::new(),
        }
    }

    fn bare_repo(
        alias: &str,
        family: Option<&str>,
        running: u32,
        failed: u32,
    ) -> FleetRepoSnapshot {
        FleetRepoSnapshot {
            alias: alias.into(),
            slug: format!("test/{alias}"),
            provider: "github".into(),
            default_branch: "main".into(),
            visibility: "private".into(),
            health_profile: "default".into(),
            status: if failed > 0 {
                "failed"
            } else if running > 0 {
                "running"
            } else {
                "green"
            }
            .into(),
            running_count: running,
            failed_count: failed,
            stale: false,
            score_badge: None,
            local: RepoLocalStatus::default(),
            latest_run: None,
            next_command: String::new(),
            family: family.map(String::from),
            last_activity_at: None,
            cache_namespace: format!("jeryu-cache-v1-test__{alias}"),
            data_namespace: format!("jeryu-data-v1-test__{alias}"),
            utilization_pressure: (running * 5).min(100) as u16,
        }
    }

    #[test]
    fn scope_items_no_families_returns_all_plus_standalone_repos() {
        let snap = make_snap(vec![
            bare_repo("nht", None, 1, 0),
            bare_repo("shared", None, 0, 0),
            bare_repo("warp", None, 0, 1),
        ]);
        let items = snap.scope_items(None);
        assert_eq!(items.len(), 4, "ALL + 3 standalone repos");
        assert_eq!(items[0].kind, FleetScopeKind::All);
        assert_eq!(items[1].kind, FleetScopeKind::Repo);
        assert_eq!(items[1].label, "nht");
        assert_eq!(items[0].rollup.running_count, 1);
        assert_eq!(items[0].rollup.failed_count, 1);
    }

    #[test]
    fn scope_items_with_family_groups_repos() {
        let snap = make_snap(vec![
            bare_repo("veox-nht", Some("veox"), 1, 0),
            bare_repo("veox-shared", Some("veox"), 0, 0),
            bare_repo("jeryu", None, 0, 0),
        ]);
        let items = snap.scope_items(None);
        // ALL + veox-* family + jeryu standalone
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, FleetScopeKind::All);
        assert_eq!(items[1].kind, FleetScopeKind::Family);
        assert_eq!(items[1].label, "veox-*");
        assert_eq!(items[2].kind, FleetScopeKind::Repo);
        assert_eq!(items[2].label, "jeryu");
    }

    #[test]
    fn drilldown_scopes_returns_family_aggregate_then_children() {
        let snap = make_snap(vec![
            bare_repo("veox-nht", Some("veox"), 1, 0),
            bare_repo("veox-shared", Some("veox"), 0, 0),
        ]);
        let items = snap.drilldown_scopes("veox");
        assert_eq!(items.len(), 3, "family aggregate + 2 children");
        assert_eq!(items[0].kind, FleetScopeKind::Family);
        assert_eq!(items[0].rollup.repo_count, 2);
        assert_eq!(items[1].kind, FleetScopeKind::Repo);
        assert_eq!(items[1].label, "veox-nht");
        assert_eq!(items[2].label, "veox-shared");
    }

    #[test]
    fn drilldown_scopes_unknown_family_returns_empty() {
        let snap = make_snap(vec![bare_repo("nht", None, 0, 0)]);
        assert!(snap.drilldown_scopes("veox").is_empty());
    }

    #[test]
    fn rollup_aggregates_running_and_failed() {
        let snap = make_snap(vec![bare_repo("a", None, 2, 0), bare_repo("b", None, 0, 1)]);
        let rollup = snap.rollup_for_indices(&[0, 1]);
        assert_eq!(rollup.running_count, 2);
        assert_eq!(rollup.failed_count, 1);
        assert_eq!(rollup.repo_count, 2);
    }

    #[test]
    fn compute_family_set_requires_two_repos() {
        let repos = vec![
            RepoConfig {
                alias: "veox-nht".into(),
                slug: "test/veox-nht".into(),
                provider: "github".into(),
                remote: "https://github.com/test/veox-nht.git".into(),
                local_root: PathBuf::from("/tmp"),
                default_branch: "main".into(),
                visibility: "private".into(),
                health_profile: "default".into(),
                family: None,
            },
            RepoConfig {
                alias: "veox-shared".into(),
                slug: "test/veox-shared".into(),
                provider: "github".into(),
                remote: "https://github.com/test/veox-shared.git".into(),
                local_root: PathBuf::from("/tmp"),
                default_branch: "main".into(),
                visibility: "private".into(),
                health_profile: "default".into(),
                family: None,
            },
            RepoConfig {
                alias: "solo-only".into(),
                slug: "test/solo-only".into(),
                provider: "github".into(),
                remote: "https://github.com/test/solo-only.git".into(),
                local_root: PathBuf::from("/tmp"),
                default_branch: "main".into(),
                visibility: "private".into(),
                health_profile: "default".into(),
                family: None,
            },
        ];
        let set = compute_family_set(&repos);
        assert!(set.contains("veox"), "veox appears twice");
        assert!(!set.contains("solo"), "solo appears only once");
    }

    #[test]
    fn derive_alias_family_splits_on_first_dash() {
        use std::collections::HashSet;
        let set: HashSet<String> = ["veox".into()].into();
        assert_eq!(derive_alias_family("veox-nht", &set), Some("veox".into()));
        assert_eq!(derive_alias_family("nht", &set), None); // no dash
        assert_eq!(derive_alias_family("other-repo", &set), None); // prefix not in set
    }

    #[test]
    fn ensure_workspace_root_default_sets_known_local_workspace() {
        // Acquire lock with poison recovery so a previous test's panic does not
        // prevent this test from running.
        let _guard = crate::test_sync::PATH_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("JERYU_WORKSPACE_ROOT").ok();
        // SAFETY: serialized by PATH_ENV_LOCK.
        unsafe {
            std::env::remove_var("JERYU_WORKSPACE_ROOT");
        }

        ensure_workspace_root_default();

        let resolved = std::env::var("JERYU_WORKSPACE_ROOT").unwrap_or_default();

        // Restore the previous environment value before the assertion so the lock
        // is not held across a potential panic.
        unsafe {
            match prev {
                Some(ref value) => std::env::set_var("JERYU_WORKSPACE_ROOT", value),
                None => std::env::remove_var("JERYU_WORKSPACE_ROOT"),
            }
        }

        // When LOCAL_WORKSPACE_ROOT_DEFAULT exists on this machine, it is the
        // first match and must be returned exactly.  In CI environments where
        // that directory does not exist, the function falls through to the
        // upward-cwd walk and returns the runner's repo root instead — assert
        // only that a valid workspace was found.
        if Path::new(LOCAL_WORKSPACE_ROOT_DEFAULT)
            .join(DEFAULT_REGISTRY_PATH)
            .exists()
        {
            assert_eq!(
                resolved, LOCAL_WORKSPACE_ROOT_DEFAULT,
                "hardcoded default should be preferred when it exists"
            );
        } else {
            assert!(
                !resolved.is_empty() && Path::new(&resolved).join(DEFAULT_REGISTRY_PATH).exists(),
                "ensure_workspace_root_default must find a workspace with {DEFAULT_REGISTRY_PATH}; got {resolved:?}"
            );
        }
    }
}
