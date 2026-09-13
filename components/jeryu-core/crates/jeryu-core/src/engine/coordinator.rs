//! Non-reentrant lock-order primitives shared by Core mutation admission.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use uuid::Uuid;

use crate::{ForgeError, Result};

/// Shared authority and immutable-repository coordination for one Core runtime.
/// These primitives do not authenticate callers or make Git and SQLite atomic.
#[derive(Debug)]
pub struct MutationCoordinator {
    authority: RwLock<()>,
    repositories: Mutex<HashMap<Uuid, Arc<Mutex<()>>>>,
    process_id: u32,
    #[cfg(test)]
    admission_entries: std::sync::atomic::AtomicUsize,
}

impl Default for MutationCoordinator {
    fn default() -> Self {
        Self {
            authority: RwLock::new(()),
            repositories: Mutex::new(HashMap::new()),
            process_id: std::process::id(),
            #[cfg(test)]
            admission_entries: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl MutationCoordinator {
    /// Hold authority read access and sorted unique repository guards throughout
    /// `operation`. Acquire Core state and SQLite only inside this closure.
    /// Do not recursively acquire this coordinator from the closure.
    pub fn with_repositories<T>(
        &self,
        repositories: &[Uuid],
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.check_process()?;
        #[cfg(test)]
        self.admission_entries
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _authority = self.authority.read();
        self.with_repository_locks(repositories, operation)
    }

    /// Hold exclusive authority access for catalog, grant or credential changes.
    /// Catalog operations may additionally name their affected repository UUIDs.
    pub fn with_authority<T>(
        &self,
        repositories: &[Uuid],
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.check_process()?;
        #[cfg(test)]
        self.admission_entries
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _authority = self.authority.write();
        self.with_repository_locks(repositories, operation)
    }

    pub(super) fn check_process(&self) -> Result<()> {
        if self.process_id != std::process::id() {
            return Err(ForgeError::WriterUnavailable(
                "a coordinator cannot be inherited by another process".to_string(),
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn admission_entries(&self) -> usize {
        self.admission_entries
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn with_repository_locks<T>(
        &self,
        repositories: &[Uuid],
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let mut ids = repositories.to_vec();
        ids.sort_unstable();
        ids.dedup();
        let locks: Vec<_> = {
            let mut registry = self.repositories.lock();
            ids.into_iter()
                .map(|id| Arc::clone(registry.entry(id).or_default()))
                .collect()
        };
        // The registry mutex is released before waiting for any repository.
        let _guards: Vec<_> = locks.iter().map(|lock| lock.lock()).collect();
        operation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_operation_keeps_authority_and_repository_guards_until_return() {
        let coordinator = MutationCoordinator::default();
        let id = Uuid::new_v4();
        coordinator
            .with_repositories(&[id, id], || {
                assert!(coordinator.authority.try_write().is_none());
                let repository = Arc::clone(coordinator.repositories.lock().get(&id).unwrap());
                assert!(repository.try_lock().is_none());
                Ok(())
            })
            .unwrap();
        assert!(coordinator.authority.try_write().is_some());
    }

    #[test]
    fn authority_operation_excludes_reads_for_its_entire_closure() {
        let coordinator = MutationCoordinator::default();
        let id = Uuid::new_v4();
        coordinator
            .with_authority(&[id], || {
                assert!(coordinator.authority.try_read().is_none());
                let repository = Arc::clone(coordinator.repositories.lock().get(&id).unwrap());
                assert!(repository.try_lock().is_none());
                Ok(())
            })
            .unwrap();
        assert!(coordinator.authority.try_read().is_some());
    }

    #[test]
    fn inherited_public_mutators_fail_before_a_held_state_lock() {
        use crate::{CreateCheckRunRequest, CreateUserRequest, ForgeCore};
        use std::time::Duration;

        let mut core = ForgeCore::new();
        Arc::get_mut(&mut core.runtime)
            .unwrap()
            .coordinator
            .process_id = std::process::id().checked_add(1).unwrap();
        let held = core.runtime.state.write();
        let (sender, receiver) = std::sync::mpsc::channel();
        let (global, repository) = (core.clone(), core.clone());
        let global_sender = sender.clone();
        let first = std::thread::spawn(move || {
            global_sender
                .send(matches!(
                    global.create_user(CreateUserRequest {
                        login: "child".into(),
                        ..Default::default()
                    }),
                    Err(ForgeError::WriterUnavailable(_))
                ))
                .unwrap();
        });
        let second = std::thread::spawn(move || {
            sender
                .send(matches!(
                    repository.create_check_run(
                        "alice",
                        "demo",
                        CreateCheckRunRequest {
                            name: "required".into(),
                            head_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                            ..Default::default()
                        }
                    ),
                    Err(ForgeError::WriterUnavailable(_))
                ))
                .unwrap();
        });
        let results = [
            receiver.recv_timeout(Duration::from_secs(5)),
            receiver.recv_timeout(Duration::from_secs(5)),
        ];
        drop(held);
        first.join().unwrap();
        second.join().unwrap();
        assert!(results.into_iter().all(|result| result == Ok(true)));
        assert!(core.runtime.state.read().users.is_empty());
    }
}
