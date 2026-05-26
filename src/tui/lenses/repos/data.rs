use std::collections::BTreeMap;

use crate::api::{
    entity::EntityKind,
    read_model::{AttentionItem, RepoFamilySummary, RepoSummary, TuiReadModel},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReposSelection {
    pub family: Option<String>,
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ReposLensInput<'a> {
    pub model: &'a TuiReadModel,
    pub selection: &'a ReposSelection,
}

pub fn select_repos_lens_input<'a>(
    model: &'a TuiReadModel,
    selection: &'a ReposSelection,
) -> ReposLensInput<'a> {
    ReposLensInput { model, selection }
}

impl<'a> ReposLensInput<'a> {
    pub fn counts(self) -> RepoFleetCounts {
        let (running, failed, aged) = self.model.repos.counts();
        RepoFleetCounts {
            repos: self.model.repos.repos.len() as u32,
            families: self.families().len() as u32,
            running,
            failed,
            aged,
        }
    }

    pub fn families(self) -> Vec<RepoFamilySummary> {
        if !self.model.repos.families.is_empty() {
            return self.model.repos.families.clone();
        }

        let mut by_family: BTreeMap<String, RepoFamilySummary> = BTreeMap::new();
        for repo in &self.model.repos.repos {
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

    pub fn repos(self) -> Vec<&'a RepoSummary> {
        match self.selection.family.as_deref() {
            Some(family) => self
                .model
                .repos
                .repos
                .iter()
                .filter(|repo| repo.family == family)
                .collect(),
            None => self.model.repos.repos.iter().collect(),
        }
    }

    pub fn selected_family(self) -> Option<RepoFamilySummary> {
        let families = self.families();
        self.selection
            .family
            .as_deref()
            .and_then(|selected| {
                families
                    .iter()
                    .find(|family| family.name == selected || family.entity.id == selected)
            })
            .cloned()
            .or_else(|| families.first().cloned())
    }

    pub fn selected_repo(self) -> Option<&'a RepoSummary> {
        self.selection
            .repo
            .as_deref()
            .and_then(|selected| {
                self.model
                    .repos
                    .repos
                    .iter()
                    .find(|repo| repo.matches_id(selected))
            })
            .or_else(|| self.repos().first().copied())
    }

    pub fn scoped_attention(self) -> Vec<&'a AttentionItem> {
        if let Some(repo) = self.selected_repo_for_explicit_scope() {
            return self
                .model
                .attention
                .iter()
                .filter(|item| attention_matches_repo(item, repo))
                .collect();
        }

        if let Some(family) = self.selection.family.as_deref() {
            let repo_entities = self
                .model
                .repos
                .repos
                .iter()
                .filter(|repo| repo.family == family)
                .map(|repo| repo.entity.clone())
                .collect::<Vec<_>>();
            return self
                .model
                .attention
                .iter()
                .filter(|item| {
                    item.entity.kind == EntityKind::RepoFamily
                        && item.entity.id == format!("family/{family}")
                        || repo_entities.iter().any(|entity| entity == &item.entity)
                })
                .collect();
        }

        self.model.attention.iter().collect()
    }

    fn selected_repo_for_explicit_scope(self) -> Option<&'a RepoSummary> {
        self.selection.repo.as_deref().and_then(|selected| {
            self.model
                .repos
                .repos
                .iter()
                .find(|repo| repo.matches_id(selected))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepoFleetCounts {
    pub repos: u32,
    pub families: u32,
    pub running: u32,
    pub failed: u32,
    pub aged: u32,
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

fn attention_matches_repo(item: &AttentionItem, repo: &RepoSummary) -> bool {
    item.entity == repo.entity
}
