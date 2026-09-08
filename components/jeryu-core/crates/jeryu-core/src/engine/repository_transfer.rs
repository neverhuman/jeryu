//! Durable two-phase repository-transfer lifecycle.

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use super::{ForgeCore, require_name};
use crate::{
    ForgeError, PrepareRepositoryTransfer, Repository, RepositoryAlias, RepositoryTransferJournal,
    RepositoryTransferStatus, Result,
};

impl ForgeCore {
    /// Return a repository by its immutable UUID.
    pub fn get_repository_by_id(&self, repository_id: Uuid) -> Result<Repository> {
        self.state
            .read()
            .repos
            .values()
            .find(|repo| repo.id == repository_id)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("repository UUID {repository_id}")))
    }

    /// Return a durable transfer by its idempotency key.
    pub fn get_repository_transfer(
        &self,
        idempotency_key: &str,
    ) -> Option<RepositoryTransferJournal> {
        self.state
            .read()
            .repository_transfers
            .get(idempotency_key)
            .cloned()
    }

    /// Resolve one old read-only slug to its canonical repository identity.
    pub fn get_repository_alias(&self, owner: &str, name: &str) -> Option<RepositoryAlias> {
        self.state
            .read()
            .repository_aliases
            .get(&(owner.to_string(), name.to_string()))
            .cloned()
    }

    /// Persist the pre-filesystem phase of a repository transfer.
    pub fn prepare_repository_transfer(
        &self,
        request: PrepareRepositoryTransfer,
    ) -> Result<RepositoryTransferJournal> {
        require_name("expected source owner", &request.expected_source_owner)?;
        require_name("expected source name", &request.expected_source_name)?;
        require_name("destination owner", &request.destination_owner)?;
        require_name("request fingerprint", &request.request_fingerprint)?;
        require_name("idempotency key", &request.idempotency_key)?;

        let mut state = self.state.write();
        if let Some(existing) = state
            .repository_transfers
            .get(&request.idempotency_key)
            .cloned()
        {
            if existing.repository_id != request.repository_id
                || existing.request_fingerprint != request.request_fingerprint
            {
                return Err(ForgeError::Conflict(format!(
                    "idempotency key {:?} is already bound to another transfer",
                    request.idempotency_key
                )));
            }
            return Ok(existing);
        }

        let repository = state
            .repos
            .values()
            .find(|repo| repo.id == request.repository_id)
            .cloned()
            .ok_or_else(|| {
                ForgeError::NotFound(format!("repository UUID {}", request.repository_id))
            })?;
        if repository.owner != request.expected_source_owner
            || repository.name != request.expected_source_name
        {
            return Err(ForgeError::Validation(format!(
                "repository source drift: expected {}/{}, found {}",
                request.expected_source_owner, request.expected_source_name, repository.full_name
            )));
        }
        if repository.owner == request.destination_owner {
            return Err(ForgeError::Validation(
                "destination owner must differ from source owner".to_string(),
            ));
        }
        let destination_key = (request.destination_owner.clone(), repository.name.clone());
        if state.repos.contains_key(&destination_key)
            || state.repository_aliases.contains_key(&destination_key)
        {
            return Err(ForgeError::Conflict(format!(
                "destination repository {}/{}",
                destination_key.0, destination_key.1
            )));
        }

        let journal = RepositoryTransferJournal {
            transaction_id: Uuid::new_v4(),
            idempotency_key: request.idempotency_key.clone(),
            request_fingerprint: request.request_fingerprint,
            repository_id: repository.id,
            source_owner: repository.owner,
            source_name: repository.name.clone(),
            destination_owner: request.destination_owner,
            destination_name: repository.name,
            status: RepositoryTransferStatus::Prepared,
            prepared_at: Utc::now(),
            completed_at: None,
            failure: None,
            receipt: None,
        };
        let previous = state.clone();
        state
            .repository_transfers
            .insert(request.idempotency_key, journal.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(journal)
    }

    /// Commit the registry phase after storage was atomically moved.
    pub fn commit_repository_transfer(
        &self,
        transaction_id: Uuid,
        receipt: Value,
    ) -> Result<RepositoryTransferJournal> {
        let mut state = self.state.write();
        let idempotency_key = transfer_key_for_id(&state, transaction_id)?;
        let journal = state
            .repository_transfers
            .get(&idempotency_key)
            .cloned()
            .expect("transfer key was found above");
        match journal.status {
            RepositoryTransferStatus::Committed => return Ok(journal),
            RepositoryTransferStatus::Failed => {
                return Err(ForgeError::Conflict(format!(
                    "repository transfer {transaction_id} already failed"
                )));
            }
            RepositoryTransferStatus::Prepared => {}
        }

        let previous = state.clone();
        if let Err(error) = super::repository_transfer_state::rekey_repository(&mut state, &journal)
        {
            *state = previous;
            return Err(error);
        }
        for alias in state.repository_aliases.values_mut() {
            if alias.repository_id == journal.repository_id {
                alias.canonical_owner = journal.destination_owner.clone();
                alias.canonical_name = journal.destination_name.clone();
            }
        }
        state.repository_aliases.insert(
            (journal.source_owner.clone(), journal.source_name.clone()),
            RepositoryAlias {
                repository_id: journal.repository_id,
                owner: journal.source_owner.clone(),
                name: journal.source_name.clone(),
                canonical_owner: journal.destination_owner.clone(),
                canonical_name: journal.destination_name.clone(),
                created_at: Utc::now(),
                transaction_id,
            },
        );
        let entry = state
            .repository_transfers
            .get_mut(&idempotency_key)
            .expect("transfer journal remains present");
        entry.status = RepositoryTransferStatus::Committed;
        entry.completed_at = Some(Utc::now());
        entry.receipt = Some(receipt);
        let committed = entry.clone();
        self.persist_after_mutation(&mut state, previous)?;
        Ok(committed)
    }

    /// Mark a prepared transfer failed after storage rollback.
    pub fn fail_repository_transfer(
        &self,
        transaction_id: Uuid,
        reason: &str,
    ) -> Result<RepositoryTransferJournal> {
        require_name("transfer failure reason", reason)?;
        let mut state = self.state.write();
        let idempotency_key = transfer_key_for_id(&state, transaction_id)?;
        let existing = state
            .repository_transfers
            .get(&idempotency_key)
            .cloned()
            .expect("transfer key was found above");
        match existing.status {
            RepositoryTransferStatus::Committed => {
                return Err(ForgeError::Conflict(format!(
                    "repository transfer {transaction_id} is already committed"
                )));
            }
            RepositoryTransferStatus::Failed => {
                if existing.failure.as_deref() == Some(reason) {
                    return Ok(existing);
                }
                return Err(ForgeError::Conflict(format!(
                    "repository transfer {transaction_id} already failed with a different reason"
                )));
            }
            RepositoryTransferStatus::Prepared => {}
        }

        let previous = state.clone();
        let journal = state
            .repository_transfers
            .get_mut(&idempotency_key)
            .expect("transfer key was found above");
        journal.status = RepositoryTransferStatus::Failed;
        journal.completed_at = Some(Utc::now());
        journal.failure = Some(reason.to_string());
        let failed = journal.clone();
        self.persist_after_mutation(&mut state, previous)?;
        Ok(failed)
    }
}

fn transfer_key_for_id(state: &super::State, transaction_id: Uuid) -> Result<String> {
    state
        .repository_transfers
        .iter()
        .find(|(_, journal)| journal.transaction_id == transaction_id)
        .map(|(key, _)| key.clone())
        .ok_or_else(|| ForgeError::NotFound(format!("repository transfer {transaction_id}")))
}
