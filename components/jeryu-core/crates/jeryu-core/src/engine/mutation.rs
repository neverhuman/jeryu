//! Admission shared by every Core writer. Existing administrative login inputs
//! remain trusted service capabilities. Credential-sensitive review and token
//! methods revalidate opaque actors inside these same authority/repository guards.

use uuid::Uuid;

use super::{ForgeCore, State};
use crate::{ForgeError, RepositoryMutationBlock, Result};

#[cfg(test)]
mod tests;

#[derive(Clone)]
struct RepositoryHint {
    owner: String,
    name: String,
    id: Uuid,
    writes: bool,
}

impl ForgeCore {
    /// Fence before touching State: a fork may inherit a permanently held lock.
    pub(super) fn validate_mutation_process(&self) -> Result<()> {
        self.runtime.coordinator.check_process()?;
        if let Some(storage) = &self.runtime.storage {
            storage.validate_writer()?;
        }
        Ok(())
    }

    pub(super) fn with_global_mutation<T>(
        &self,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.with_installation_custody(|| {
            self.require_ordinary_mutation()?;
            operation()
        })
    }

    /// Process-local installation hooks and custody readback must remain usable
    /// when reopening an interrupted commissioning operation. This guard alone
    /// never admits an ordinary durable or external mutation.
    pub(super) fn with_installation_custody<T>(
        &self,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        self.runtime.coordinator.with_authority(&[], operation)
    }

    /// Call only inside the authority/repository guards, before State or any
    /// external effect. Reservation needs authority-write, so the barrier cannot
    /// appear between this check and the guarded callback's completion.
    pub(super) fn require_ordinary_mutation(&self) -> Result<()> {
        if let Some(storage) = &self.runtime.storage {
            storage.require_ordinary_mutation()?;
        }
        Ok(())
    }

    pub(super) fn with_repository_mutation<T>(
        &self,
        owner: &str,
        repo: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.with_named_mutation(&[(owner, repo)], false, operation)
    }

    pub(super) fn with_repository_authority_mutation<T>(
        &self,
        owner: &str,
        repo: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.with_named_mutation(&[(owner, repo)], true, operation)
    }

    pub(super) fn with_profile_mutation<T>(
        &self,
        owner: &str,
        repo: &str,
        login: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        // A new display profile is a catalog mutation. Existing profiles need
        // only repository admission; there is no public profile-deletion API.
        let creates_profile = !self.runtime.state.read().users.contains_key(login);
        self.with_named_mutation(&[(owner, repo)], creates_profile, operation)
    }

    pub(super) fn with_source_mutation<T>(
        &self,
        owner: &str,
        repo: &str,
        source: Option<&str>,
        author: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        let creates_profile = !self.runtime.state.read().users.contains_key(author);
        let mut repositories = vec![(owner, repo)];
        if let Some(source) = source.map(str::trim).filter(|source| !source.is_empty()) {
            repositories.push(source.split_once('/').ok_or_else(|| {
                ForgeError::Validation("source repository must be owner/name".to_string())
            })?);
        }
        self.with_named_mutation(&repositories, creates_profile, operation)
    }

    pub(super) fn with_pull_mutation<T>(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        let key = (owner.to_string(), repo.to_string(), number);
        let source = self
            .runtime
            .state
            .read()
            .pulls
            .get(&key)
            .ok_or_else(|| ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}")))?
            .source_repository
            .clone();
        let mut repositories = vec![(owner, repo)];
        if !source.is_empty() {
            repositories.push(source.split_once('/').ok_or_else(|| {
                ForgeError::Validation("stored source repository must be owner/name".to_string())
            })?);
        }
        self.with_named_mutation(&repositories, false, || {
            let matches = self
                .runtime
                .state
                .read()
                .pulls
                .get(&key)
                .is_some_and(|pr| pr.source_repository == source);
            if !matches {
                return Err(ForgeError::Conflict(
                    "pull request source identity changed".to_string(),
                ));
            }
            operation()
        })
    }

    fn with_named_mutation<T>(
        &self,
        names: &[(&str, &str)],
        authority: bool,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        let hints = {
            let state = self.runtime.state.read();
            names
                .iter()
                .enumerate()
                .map(|(index, (owner, name))| {
                    let repo = state
                        .repos
                        .get(&(owner.to_string(), name.to_string()))
                        .ok_or_else(|| {
                            ForgeError::NotFound(format!("repository {owner}/{name}"))
                        })?;
                    Ok(RepositoryHint {
                        owner: owner.to_string(),
                        name: name.to_string(),
                        id: repo.id,
                        writes: index == 0,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        };
        let ids = hints.iter().map(|hint| hint.id).collect::<Vec<_>>();
        let admitted = || {
            self.require_ordinary_mutation()?;
            let state = self.runtime.state.read();
            for hint in &hints {
                if state
                    .repos
                    .get(&(hint.owner.clone(), hint.name.clone()))
                    .map(|repo| repo.id)
                    != Some(hint.id)
                {
                    return Err(ForgeError::Conflict(
                        "repository identity changed before admission".to_string(),
                    ));
                }
                require_repository_admissible(&state, hint.id, hint.writes)?;
            }
            drop(state);
            operation()
        };
        if authority {
            self.runtime.coordinator.with_authority(&ids, admitted)
        } else {
            self.runtime.coordinator.with_repositories(&ids, admitted)
        }
    }

    pub(super) fn with_repository_id_mutation<T>(
        &self,
        id: Uuid,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        self.runtime.coordinator.with_authority(&[id], || {
            self.require_ordinary_mutation()?;
            require_repository_writable(&self.runtime.state.read(), id)?;
            operation()
        })
    }

    /// Creation reserves a new UUID under the catalog and repository guards.
    /// A retained creation retry must also pass current repository custody.
    pub(super) fn with_repository_creation_mutation<T>(
        &self,
        id: Uuid,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        self.runtime.coordinator.with_authority(&[id], || {
            let state = self.runtime.state.read();
            if state.repos.values().any(|repo| repo.id == id) {
                require_repository_writable(&state, id)?;
            }
            drop(state);
            operation()
        })
    }

    pub(super) fn with_transfer_prepare_mutation<T>(
        &self,
        repository_id: Uuid,
        idempotency_key: &str,
        request_fingerprint: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        self.runtime
            .coordinator
            .with_authority(&[repository_id], || {
                self.require_ordinary_mutation()?;
                let state = self.runtime.state.read();
                // A bound key cannot be substituted, even with an unknown UUID.
                // The exclusive authority gate stabilizes this conflict lookup;
                // it neither reads nor mutates the other repository's contents.
                if let Some(existing) = state.repository_transfers.get(idempotency_key)
                    && (existing.repository_id != repository_id
                        || existing.request_fingerprint != request_fingerprint)
                {
                    return Err(ForgeError::Conflict(format!(
                        "idempotency key {idempotency_key:?} is already bound to another transfer"
                    )));
                }
                // Even an identical replay must pass current repository custody.
                require_repository_writable(&state, repository_id)?;
                drop(state);
                operation()
            })
    }

    pub(super) fn with_transfer_mutation<T>(
        &self,
        transaction_id: Uuid,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        let id = self
            .runtime
            .state
            .read()
            .repository_transfers
            .values()
            .find(|transfer| transfer.transaction_id == transaction_id)
            .map(|transfer| transfer.repository_id)
            .ok_or_else(|| ForgeError::NotFound(format!("repository transfer {transaction_id}")))?;
        self.with_repository_id_mutation(id, || {
            let matches = self
                .runtime
                .state
                .read()
                .repository_transfers
                .values()
                .any(|transfer| {
                    transfer.transaction_id == transaction_id && transfer.repository_id == id
                });
            if !matches {
                return Err(ForgeError::Conflict(
                    "repository transfer identity changed".to_string(),
                ));
            }
            operation()
        })
    }

    pub(super) fn with_audit_mutation<T>(
        &self,
        subject: &str,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.validate_mutation_process()?;
        // Audit subjects outlive deleted repositories. Resolve a hint and then
        // revalidate it under authority and the still-catalogued UUID guard.
        let key = subject
            .split_once('/')
            .map(|(owner, name)| (owner.to_string(), name.to_string()));
        let resolve = |state: &State| {
            key.as_ref().and_then(|key| {
                state.repos.get(key).map(|repo| repo.id).or_else(|| {
                    state
                        .repository_aliases
                        .get(key)
                        .map(|alias| alias.repository_id)
                })
            })
        };
        let hint = resolve(&self.runtime.state.read());
        let ids = hint.into_iter().collect::<Vec<_>>();
        self.runtime.coordinator.with_authority(&ids, || {
            self.require_ordinary_mutation()?;
            let state = self.runtime.state.read();
            if resolve(&state) != hint {
                return Err(ForgeError::Conflict(
                    "audit subject identity changed before admission".to_string(),
                ));
            }
            if let Some(id) = hint {
                require_repository_writable(&state, id)?;
            }
            drop(state);
            operation()
        })
    }

    /// Record an irreversible admission restriction. This trusted maintenance
    /// interface cannot clear a block, substitute a different disposition or
    /// confer authenticated review authority on its supplied evidence string.
    pub fn block_repository_mutations(
        &self,
        repository_id: Uuid,
        block: RepositoryMutationBlock,
    ) -> Result<RepositoryMutationBlock> {
        self.validate_mutation_process()?;
        match &block {
            RepositoryMutationBlock::ReadOnly { reason, evidence }
                if reason.trim().is_empty() || evidence.trim().is_empty() =>
            {
                return Err(ForgeError::Validation(
                    "read-only custody requires reason and evidence".to_string(),
                ));
            }
            RepositoryMutationBlock::ReconciliationRequired {
                operation_id,
                reason,
            } if operation_id.is_nil() || reason.trim().is_empty() => {
                return Err(ForgeError::Validation(
                    "reconciliation requires operation ID and reason".to_string(),
                ));
            }
            _ => {}
        }
        self.runtime
            .coordinator
            .with_authority(&[repository_id], || {
                self.require_ordinary_mutation()?;
                let mut state = self.runtime.state.write();
                if !state.repos.values().any(|repo| repo.id == repository_id) {
                    return Err(ForgeError::NotFound(format!(
                        "repository UUID {repository_id}"
                    )));
                }
                if let Some(existing) = state.repository_mutation_blocks.get(&repository_id) {
                    return if existing == &block {
                        Ok(existing.clone())
                    } else {
                        Err(ForgeError::Conflict(
                            "repository mutation restriction is already recorded".to_string(),
                        ))
                    };
                }
                let previous = state.clone();
                state
                    .repository_mutation_blocks
                    .insert(repository_id, block.clone());
                self.persist_after_mutation(&mut state, previous)?;
                Ok(block)
            })
    }

    pub fn repository_mutation_block(
        &self,
        repository_id: Uuid,
    ) -> Result<Option<RepositoryMutationBlock>> {
        self.runtime.coordinator.check_process()?;
        let state = self.runtime.state.read();
        if !state.repos.values().any(|repo| repo.id == repository_id) {
            return Err(ForgeError::NotFound(format!(
                "repository UUID {repository_id}"
            )));
        }
        Ok(state
            .repository_mutation_blocks
            .get(&repository_id)
            .cloned())
    }
}

fn require_repository_writable(state: &State, repository_id: Uuid) -> Result<()> {
    require_repository_admissible(state, repository_id, true)
}

pub(super) fn require_repository_admissible(
    state: &State,
    repository_id: Uuid,
    writes: bool,
) -> Result<()> {
    let repo = state
        .repos
        .values()
        .find(|repo| repo.id == repository_id)
        .ok_or_else(|| ForgeError::NotFound(format!("repository UUID {repository_id}")))?;
    if writes && (repo.archived || repo.disabled) {
        return Err(ForgeError::Forbidden(format!(
            "repository {} is archived or disabled",
            repo.full_name
        )));
    }
    match state.repository_mutation_blocks.get(&repository_id) {
        Some(RepositoryMutationBlock::ReadOnly { reason, .. }) if writes => {
            Err(ForgeError::Forbidden(format!(
                "repository {} is read-only: {reason}",
                repo.full_name
            )))
        }
        Some(RepositoryMutationBlock::ReconciliationRequired {
            operation_id,
            reason,
        }) => Err(ForgeError::WriterUnavailable(format!(
            "repository {} requires reconciliation of {operation_id}: {reason}",
            repo.full_name
        ))),
        Some(RepositoryMutationBlock::ReadOnly { .. }) | None => Ok(()),
    }
}
