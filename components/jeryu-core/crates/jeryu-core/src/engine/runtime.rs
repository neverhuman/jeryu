//! One process-local snapshot and lease for each admitted backing-store identity.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, Weak};

use parking_lot::{Mutex, MutexGuard, RwLock};

use super::State;
use super::coordinator::MutationCoordinator;
use super::storage::SqliteStore;
use super::writer::{BackingIdentity, WriterLease};
use crate::{ForgeError, Result};

#[derive(Debug, Default)]
pub(super) struct SharedRuntime {
    pub(super) state: RwLock<State>,
    pub(super) storage: Option<SqliteStore>,
    pub(super) coordinator: MutationCoordinator,
    pub(super) storage_root: Option<PathBuf>,
    pub(super) review_git_observer: RwLock<Option<Arc<dyn super::ReviewGitObserver>>>,
    pub(super) required_publisher_authority:
        RwLock<Option<Arc<dyn super::RequiredPublisherAuthority>>>,
}

type Registry = HashMap<BackingIdentity, Weak<SharedRuntime>>;

struct RuntimeRegistry {
    process_id: AtomicU32,
    runtimes: OnceLock<Mutex<Registry>>,
}

impl RuntimeRegistry {
    const fn new() -> Self {
        Self {
            process_id: AtomicU32::new(0),
            runtimes: OnceLock::new(),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Registry>> {
        let process_id = std::process::id();
        if let Err(owner) =
            self.process_id
                .compare_exchange(0, process_id, Ordering::AcqRel, Ordering::Acquire)
            && owner != process_id
        {
            return Err(ForgeError::WriterUnavailable(
                "a runtime registry cannot be inherited by another process".to_string(),
            ));
        }
        // Fence before either synchronization primitive: fork can inherit a held
        // mutex or an in-progress OnceLock initialization from a vanished thread.
        Ok(self.runtimes.get_or_init(Mutex::default).lock())
    }
}

static RUNTIMES: RuntimeRegistry = RuntimeRegistry::new();

pub(super) fn open(database: &Path, storage_root: Option<&Path>) -> Result<Arc<SharedRuntime>> {
    // This startup-only mutex serializes acquisition, migration and publication.
    // Operation paths never acquire it, and it owns no strong runtime references.
    let mut registry = RUNTIMES.lock()?;
    let identity = BackingIdentity::admit(database, storage_root)?;
    registry.retain(|_, runtime| runtime.strong_count() > 0);
    if let Some(runtime) = registry.get(&identity).and_then(Weak::upgrade) {
        runtime
            .storage
            .as_ref()
            .expect("persistent runtime")
            .validate_writer()?;
        return Ok(runtime);
    }
    for active in registry.keys() {
        if active.database == identity.database
            || (identity.storage_root.is_some() && active.storage_root == identity.storage_root)
        {
            return Err(ForgeError::WriterUnavailable(
                "a backing resource is already bound to another active runtime".to_string(),
            ));
        }
    }
    let writer = Arc::new(WriterLease::acquire(&identity)?);
    let (storage, state) = SqliteStore::open(&identity.database, writer)?;
    let runtime = Arc::new(SharedRuntime {
        state: RwLock::new(state),
        storage: Some(storage),
        coordinator: MutationCoordinator::default(),
        storage_root: identity.storage_root.clone(),
        review_git_observer: RwLock::new(None),
        required_publisher_authority: RwLock::new(None),
    });
    registry.insert(identity, Arc::downgrade(&runtime));
    Ok(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn inherited_registry_is_rejected_before_waiting_on_its_mutex() {
        let registry = Arc::new(RuntimeRegistry::new());
        let owner = std::process::id().checked_add(1).unwrap();
        registry.process_id.store(owner, Ordering::Release);
        let held = registry.runtimes.get_or_init(Mutex::default).lock();
        let (sender, receiver) = std::sync::mpsc::channel();
        let inherited = Arc::clone(&registry);
        let thread = std::thread::spawn(move || {
            sender
                .send(matches!(
                    inherited.lock(),
                    Err(ForgeError::WriterUnavailable(_))
                ))
                .unwrap();
        });
        assert!(receiver.recv_timeout(Duration::from_secs(5)).unwrap());
        drop(held);
        thread.join().unwrap();
    }

    #[test]
    fn inherited_registry_is_rejected_before_initializing_synchronization() {
        let registry = RuntimeRegistry::new();
        registry.process_id.store(
            std::process::id().checked_add(1).unwrap(),
            Ordering::Release,
        );
        assert!(matches!(
            registry.lock(),
            Err(ForgeError::WriterUnavailable(_))
        ));
        assert!(registry.runtimes.get().is_none());
    }
}
