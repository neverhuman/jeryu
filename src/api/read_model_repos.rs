use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::api::entity::{EntityKind, EntityRef};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReposSnapshot {
    pub registry_path: String,
    pub families: Vec<RepoFamilySummary>,
    pub repos: Vec<RepoSummary>,
}

impl ReposSnapshot {
    pub fn from_fleet_snapshot(snapshot: &crate::repo_fleet::FleetSnapshot) -> Self {
        Self::from_repo_summaries(
            snapshot.registry_path.clone(),
            snapshot
                .repos
                .iter()
                .map(RepoSummary::from_fleet_repo)
                .collect(),
        )
    }

    pub fn from_registry(
        registry_path: impl Into<String>,
        registry: &crate::repo_fleet::RepoRegistry,
    ) -> Self {
        Self::from_repo_summaries(
            registry_path.into(),
            registry
                .repo
                .iter()
                .map(RepoSummary::from_repo_config)
                .collect(),
        )
    }

    pub fn from_repo_summaries(registry_path: impl Into<String>, repos: Vec<RepoSummary>) -> Self {
        let families = family_summaries(&repos);
        Self {
            registry_path: registry_path.into(),
            families,
            repos,
        }
    }

    pub fn counts(&self) -> (u32, u32, u32) {
        let running = self.repos.iter().map(|repo| repo.running_count).sum();
        let failed = self.repos.iter().map(|repo| repo.failed_count).sum();
        let aged = self.repos.iter().filter(|repo| repo.aged).count() as u32;
        (running, failed, aged)
    }

    pub fn repos_for_family(&self, family: &str) -> Vec<&RepoSummary> {
        self.repos
            .iter()
            .filter(|repo| repo.family == family)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoFamilySummary {
    pub entity: EntityRef,
    pub name: String,
    pub status: String,
    pub repo_count: u32,
    pub running_count: u32,
    pub failed_count: u32,
    pub aged_count: u32,
}

impl RepoFamilySummary {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            entity: EntityRef::new(EntityKind::RepoFamily, format!("family/{name}")),
            name,
            status: "unknown".into(),
            repo_count: 0,
            running_count: 0,
            failed_count: 0,
            aged_count: 0,
        }
    }
}

impl Default for RepoFamilySummary {
    fn default() -> Self {
        Self::new("unknown")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoSummary {
    pub entity: EntityRef,
    pub family: String,
    pub alias: String,
    pub slug: String,
    pub provider: String,
    pub default_branch: String,
    pub visibility: String,
    pub health_profile: String,
    pub status: String,
    pub running_count: u32,
    pub failed_count: u32,
    pub aged: bool,
    pub score_badge: Option<String>,
    pub local_branch: Option<String>,
    pub local_sha: Option<String>,
    pub dirty: bool,
    pub next_command: String,
}

impl RepoSummary {
    pub fn new(alias: impl Into<String>, slug: impl Into<String>) -> Self {
        let alias = alias.into();
        let slug = slug.into();
        Self {
            entity: EntityRef::new(EntityKind::Repo, slug.clone()),
            family: infer_family(&slug),
            alias,
            slug,
            provider: "unknown".into(),
            default_branch: "main".into(),
            visibility: "unknown".into(),
            health_profile: "default".into(),
            status: "unknown".into(),
            running_count: 0,
            failed_count: 0,
            aged: false,
            score_badge: None,
            local_branch: None,
            local_sha: None,
            dirty: false,
            next_command: String::new(),
        }
    }

    pub fn matches_id(&self, id: &str) -> bool {
        self.alias == id || self.slug == id || self.entity.id == id
    }

    pub fn from_fleet_repo(repo: &crate::repo_fleet::FleetRepoSnapshot) -> Self {
        Self {
            entity: EntityRef::new(EntityKind::Repo, repo.slug.clone()),
            family: infer_family(&repo.slug),
            alias: repo.alias.clone(),
            slug: repo.slug.clone(),
            provider: repo.provider.clone(),
            default_branch: repo.default_branch.clone(),
            visibility: repo.visibility.clone(),
            health_profile: repo.health_profile.clone(),
            status: repo.status.clone(),
            running_count: repo.running_count,
            failed_count: repo.failed_count,
            aged: repo.aged(),
            score_badge: repo.score_badge.clone(),
            local_branch: repo.local.branch.clone(),
            local_sha: repo.local.sha_short.clone(),
            dirty: repo.local.dirty,
            next_command: repo.next_command.clone(),
        }
    }

    pub fn from_repo_config(repo: &crate::repo_fleet::RepoConfig) -> Self {
        let local = crate::repo_fleet::local_git_status(repo);
        Self {
            entity: EntityRef::new(EntityKind::Repo, repo.slug.clone()),
            family: infer_family(&repo.slug),
            alias: repo.alias.clone(),
            slug: repo.slug.clone(),
            provider: repo.provider.clone(),
            default_branch: repo.default_branch.clone(),
            visibility: repo.visibility.clone(),
            health_profile: repo.health_profile.clone(),
            status: local_status_label(&local),
            running_count: 0,
            failed_count: 0,
            aged: false,
            score_badge: None,
            local_branch: local.branch,
            local_sha: local.sha_short,
            dirty: local.dirty,
            next_command: format!("cd {} && just fast", repo.local_root.display()),
        }
    }
}

impl Default for RepoSummary {
    fn default() -> Self {
        Self::new("unknown", "unknown/unknown")
    }
}

fn infer_family(slug: &str) -> String {
    slug.split('/')
        .next()
        .filter(|family| !family.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn family_summaries(repos: &[RepoSummary]) -> Vec<RepoFamilySummary> {
    let mut by_family: BTreeMap<String, RepoFamilySummary> = BTreeMap::new();
    for repo in repos {
        let entry = by_family
            .entry(repo.family.clone())
            .or_insert_with(|| RepoFamilySummary::new(&repo.family));
        entry.repo_count += 1;
        entry.running_count += repo.running_count;
        entry.failed_count += repo.failed_count;
        if repo.aged {
            entry.aged_count += 1;
        }
        entry.status = family_status(entry);
    }
    by_family.into_values().collect()
}

fn family_status(family: &RepoFamilySummary) -> String {
    if family.failed_count > 0 {
        "failed".into()
    } else if family.running_count > 0 {
        "running".into()
    } else if family.aged_count > 0 {
        "aged".into()
    } else {
        "green".into()
    }
}

fn local_status_label(local: &crate::repo_fleet::RepoLocalStatus) -> String {
    if !local.exists {
        "missing".into()
    } else if local.dirty {
        "dirty".into()
    } else {
        "green".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_summary_uses_repo_entity_kind() {
        let repo = RepoSummary::new("core", "neverhuman/jeryu");
        assert_eq!(repo.entity.kind, EntityKind::Repo);
        assert_eq!(repo.family, "neverhuman");
    }

    #[test]
    fn repos_snapshot_counts_statuses() {
        let mut good = RepoSummary::new("core", "neverhuman/jeryu");
        good.running_count = 1;
        let mut bad = RepoSummary::new("shared", "neverhuman/shared");
        bad.failed_count = 2;
        bad.aged = true;
        let snapshot = ReposSnapshot {
            registry_path: ".jeryu/repos.toml".into(),
            families: vec![RepoFamilySummary::new("neverhuman")],
            repos: vec![good, bad],
        };

        assert_eq!(snapshot.counts(), (1, 2, 1));
        assert_eq!(snapshot.repos_for_family("neverhuman").len(), 2);
    }

    #[test]
    fn repos_snapshot_projects_from_fleet_snapshot() {
        let snapshot = crate::repo_fleet::FleetSnapshot {
            generated_at: "2026-05-26T00:00:00Z".into(),
            registry_path: ".jeryu/repos.toml".into(),
            repos: vec![crate::repo_fleet::FleetRepoSnapshot::projection_fixture(
                "core",
                "neverhuman/jeryu",
                true,
            )],
            events: Vec::new(),
        };

        let repos = ReposSnapshot::from_fleet_snapshot(&snapshot);

        assert_eq!(repos.registry_path, ".jeryu/repos.toml");
        assert_eq!(repos.repos[0].entity.kind, EntityKind::Repo);
        assert_eq!(repos.repos[0].family, "neverhuman");
        assert_eq!(repos.repos[0].local_branch.as_deref(), Some("main"));
        assert_eq!(repos.repos[0].score_badge.as_deref(), Some("89"));
        assert!(repos.repos[0].aged);
        assert_eq!(repos.families[0].failed_count, 2);
    }

    #[test]
    fn repo_summary_serializes_only_aged_freshness() {
        let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
        repo.aged = true;
        let encoded = serde_json::to_value(&repo).expect("serialize repo");

        assert_eq!(encoded["aged"], true);
    }

    #[test]
    fn repos_snapshot_round_trips_json() {
        let snapshot = ReposSnapshot {
            registry_path: ".jeryu/repos.toml".into(),
            families: vec![RepoFamilySummary::new("neverhuman")],
            repos: vec![RepoSummary::new("core", "neverhuman/jeryu")],
        };

        let encoded = serde_json::to_string(&snapshot).expect("serialize repos");
        let decoded: ReposSnapshot = serde_json::from_str(&encoded).expect("deserialize repos");
        assert_eq!(decoded, snapshot);
    }
}
