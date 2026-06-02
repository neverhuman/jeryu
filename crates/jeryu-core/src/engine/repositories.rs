//! Repositories and labels.

use chrono::Utc;
use uuid::Uuid;

use super::{Counters, ForgeCore, require_name};
use crate::errors::{ForgeError, Result};
use crate::model::*;

impl ForgeCore {
    pub fn create_repository(
        &self,
        owner: &str,
        request: CreateRepositoryRequest,
    ) -> Result<Repository> {
        require_name("repository name", &request.name)?;
        let mut state = self.state.write();
        let key = (owner.to_string(), request.name.clone());
        if state.repos.contains_key(&key) {
            return Err(ForgeError::Conflict(format!(
                "repository {owner}/{}",
                request.name
            )));
        }
        let previous = state.clone();
        let now = Utc::now();
        let repo = Repository {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            name: request.name.clone(),
            full_name: format!("{owner}/{}", request.name),
            private: request.private,
            description: request.description,
            default_branch: request.default_branch.unwrap_or_else(|| "main".to_string()),
            archived: false,
            disabled: false,
            created_at: now,
            updated_at: now,
        };
        state.counters.insert(key.clone(), Counters::default());
        state.repos.insert(key, repo.clone());
        super::ensure_default_branch_protection(&mut state, &repo);
        self.persist_after_mutation(&mut state, previous)?;
        drop(state);
        if let Some(materializer) = &self.repo_materializer {
            materializer.materialize(owner, &repo.name, &repo.default_branch)?;
        }
        Ok(repo)
    }

    pub fn list_repositories(&self, owner: Option<&str>) -> Vec<Repository> {
        let mut repos: Vec<_> = self
            .state
            .read()
            .repos
            .values()
            .filter(|repo| owner.is_none_or(|owner| repo.owner == owner))
            .cloned()
            .collect();
        repos.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        repos
    }

    pub fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        self.state
            .read()
            .repos
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("repository {owner}/{repo}")))
    }

    pub fn create_label(
        &self,
        owner: &str,
        repo: &str,
        request: CreateLabelRequest,
    ) -> Result<Label> {
        require_name("label name", &request.name)?;
        self.ensure_repo_exists(owner, repo)?;
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), request.name.clone());
        if state.labels.contains_key(&key) {
            return Err(ForgeError::Conflict(format!(
                "label {owner}/{repo}/{}",
                request.name
            )));
        }
        let previous = state.clone();
        let label = Label {
            id: Uuid::new_v4(),
            name: request.name,
            color: request.color,
            description: request.description,
        };
        state.labels.insert(key, label.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(label)
    }

    pub fn list_labels(&self, owner: &str, repo: &str) -> Result<Vec<Label>> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.state.read();
        let mut labels: Vec<_> = state
            .labels
            .iter()
            .filter(|((label_owner, label_repo, _), _)| label_owner == owner && label_repo == repo)
            .map(|(_, label)| label.clone())
            .collect();
        labels.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(labels)
    }
}
