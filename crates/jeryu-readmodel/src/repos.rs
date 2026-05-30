//! Repos read-model contract: tracked repositories and their families.
//!
//! Constructors that bridged the product's fleet/registry layer live in the
//! daemon/assembler, not here — this crate carries only the pure contract +
//! the in-crate aggregation helpers.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::entity::{EntityKind, EntityRef};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReposSnapshot {
    pub registry_path: String,
    pub families: Vec<RepoFamilySummary>,
    pub repos: Vec<RepoSummary>,
}

impl ReposSnapshot {
    /// Build a snapshot from repo summaries, deriving family rollups.
    pub fn from_repo_summaries(registry_path: impl Into<String>, repos: Vec<RepoSummary>) -> Self {
        let families = family_summaries(&repos);
        Self {
            registry_path: registry_path.into(),
            families,
            repos,
        }
    }

    /// `(running, failed, aged)` rollup across all repos.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_rollup_aggregates_counts() {
        let mut a = RepoSummary::new("web", "core/web");
        a.running_count = 2;
        let mut b = RepoSummary::new("api", "core/api");
        b.failed_count = 1;
        let snapshot = ReposSnapshot::from_repo_summaries("/reg", vec![a, b]);

        assert_eq!(snapshot.families.len(), 1);
        let fam = &snapshot.families[0];
        assert_eq!(fam.name, "core");
        assert_eq!(fam.repo_count, 2);
        assert_eq!(fam.running_count, 2);
        assert_eq!(fam.failed_count, 1);
        assert_eq!(fam.status, "failed");
        assert_eq!(snapshot.counts(), (2, 1, 0));
    }

    #[test]
    fn repo_matches_by_alias_slug_or_id() {
        let repo = RepoSummary::new("web", "core/web");
        assert!(repo.matches_id("web"));
        assert!(repo.matches_id("core/web"));
    }

    #[test]
    fn repos_snapshot_round_trips_json() {
        let snapshot =
            ReposSnapshot::from_repo_summaries("/reg", vec![RepoSummary::new("web", "core/web")]);
        let encoded = serde_json::to_string(&snapshot).unwrap();
        let decoded: ReposSnapshot = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, snapshot);
    }
}
