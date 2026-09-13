//! Durable identity for repository metadata and its recoverable materialization.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Counters, ForgeCore, require_name};
use crate::{CreateRepositoryRequest, ForgeError, Repository, Result};

/// A retained creation request. Completed records also prevent UUID reuse after
/// deletion. This receipt covers Core's materializer, not later browser setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryCreation {
    pub repository_id: Uuid,
    pub owner: String,
    pub request: CreateRepositoryRequest,
    pub materialized: bool,
}

impl ForgeCore {
    /// Create with a fresh identity. Durable callers should reserve a UUID and
    /// use `create_repository_with_id` to retry an interrupted request.
    pub fn create_repository(
        &self,
        owner: &str,
        request: CreateRepositoryRequest,
    ) -> Result<Repository> {
        self.create_repository_with_id(Uuid::new_v4(), owner, request)
    }

    /// Commit metadata and the exact request together, then materialize Git.
    /// Reusing the UUID resumes only that request and original repository. A
    /// failed materializer or completion write remains visible after restart.
    pub fn create_repository_with_id(
        &self,
        repository_id: Uuid,
        owner: &str,
        request: CreateRepositoryRequest,
    ) -> Result<Repository> {
        self.with_repository_creation_mutation(repository_id, || {
            self.create_repository_with_id_admitted(repository_id, owner, request)
        })
    }

    fn create_repository_with_id_admitted(
        &self,
        repository_id: Uuid,
        owner: &str,
        request: CreateRepositoryRequest,
    ) -> Result<Repository> {
        require_name("repository name", &request.name)?;
        if repository_id.is_nil() {
            return Err(ForgeError::Validation(
                "creation UUID must not be nil".into(),
            ));
        }
        if let Some(materializer) = &self.repo_materializer {
            materializer.validate(
                owner,
                &request.name,
                request.default_branch.as_deref().unwrap_or("main"),
            )?;
        }
        let key = (owner.to_owned(), request.name.clone());
        let mut state = self.runtime.state.write();
        let repo = if let Some(journal) = state.repository_creations.get(&repository_id) {
            if journal.owner != owner || journal.request != request {
                return Err(ForgeError::Conflict(
                    "creation UUID belongs to another request".into(),
                ));
            }
            let repo = state
                .repos
                .get(&key)
                .filter(|repo| repo.id == repository_id)
                .ok_or_else(|| {
                    ForgeError::Conflict("original repository was removed or moved".into())
                })?
                .clone();
            if journal.materialized {
                return Ok(repo);
            }
            repo
        } else {
            if state.repos.contains_key(&key)
                || state.repos.values().any(|repo| repo.id == repository_id)
            {
                return Err(ForgeError::Conflict(format!(
                    "repository {owner}/{}",
                    request.name
                )));
            }
            let previous = state.clone();
            let now = Utc::now();
            let repo = Repository {
                id: repository_id,
                owner: owner.to_owned(),
                name: request.name.clone(),
                full_name: format!("{owner}/{}", request.name),
                private: request.private,
                description: request.description.clone(),
                default_branch: request
                    .default_branch
                    .clone()
                    .unwrap_or_else(|| "main".into()),
                family: None,
                archived: false,
                disabled: false,
                created_at: now,
                updated_at: now,
            };
            state.counters.insert(key.clone(), Counters::default());
            state.repos.insert(key.clone(), repo.clone());
            super::ensure_default_branch_protection(&mut state, &repo);
            state.repository_creations.insert(
                repository_id,
                RepositoryCreation {
                    repository_id,
                    owner: owner.to_owned(),
                    request,
                    materialized: false,
                },
            );
            self.persist_after_mutation(&mut state, previous)?;
            repo
        };
        drop(state);
        if let Some(materializer) = &self.repo_materializer {
            materializer.materialize_repository(&repo)?;
        }
        let mut state = self.runtime.state.write();
        let current = state
            .repos
            .get(&key)
            .filter(|current| current.id == repository_id)
            .ok_or_else(|| ForgeError::Conflict("repository changed during creation".into()))?
            .clone();
        let previous = state.clone();
        state
            .repository_creations
            .get_mut(&repository_id)
            .expect("creation journal is retained across mutations")
            .materialized = true;
        self.persist_after_mutation(&mut state, previous)?;
        Ok(current)
    }

    /// Inspect retained receipts, including unfinished and deleted identities.
    /// Authentication and repository visibility remain the transport's duty.
    pub fn repository_creations(&self) -> Vec<RepositoryCreation> {
        let mut journals: Vec<_> = self
            .runtime
            .state
            .read()
            .repository_creations
            .values()
            .cloned()
            .collect();
        journals.sort_by_key(|journal| journal.repository_id);
        journals
    }
}

#[cfg(test)]
mod tests;
