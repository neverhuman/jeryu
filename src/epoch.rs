//! Owner: Epoch-Based Cache Invalidation
//! Proof: `cargo test -p jeryu -- epoch`
//! Invariants: Epoch bumps are recorded with author_job_id and reason; is_valid fails closed (returns false) on lookup failure; epochs are per-scope and never shared across scopes

use anyhow::Result;
use cache_brain_adapter::{AdapterBackend, AnyPool};

use crate::state::StateBackend;

fn adapter_backend(backend: StateBackend) -> AdapterBackend {
    match backend {
        StateBackend::RedlineDb => AdapterBackend::RedlineDb,
        StateBackend::CompatSql => AdapterBackend::CompatSql,
    }
}

/// Manages epoch-based cache invalidation.
///
/// Instead of scanning and deleting massive graphs of files on disk,
/// we simply bump an epoch pointer. Any cache lookups for objects
/// tied to an older epoch strictly fail, immediately isolating poisoned trees.
#[derive(Clone)]
pub struct EpochManager {
    pool: AnyPool,
    backend: StateBackend,
}

impl EpochManager {
    pub fn new(pool: AnyPool) -> Self {
        Self::with_backend(pool, StateBackend::RedlineDb)
    }

    pub fn with_backend(pool: AnyPool, backend: StateBackend) -> Self {
        Self { pool, backend }
    }

    /// Retrieve the current epoch for a given boundary scope (e.g., "global", "project:123", "runner:456").
    pub async fn get_epoch(&self, scope: &str) -> Result<u64> {
        let epoch = cache_brain_adapter::current_epoch_for(
            &self.pool,
            adapter_backend(self.backend),
            scope,
        )
        .await?;
        Ok(epoch as u64)
    }

    /// Bump the epoch, instantly invalidating all cache entries tied to the previous epoch.
    pub async fn bump_epoch(&self, scope: &str, author_job_id: i64, reason: &str) -> Result<u64> {
        let current = self.get_epoch(scope).await?;
        let next = current + 1;

        cache_brain_adapter::upsert_epoch_for(
            &self.pool,
            adapter_backend(self.backend),
            scope,
            next as i64,
            &chrono::Utc::now().to_rfc3339(),
            author_job_id,
            reason,
        )
        .await?;

        tracing::warn!(
            "Escalated cache epoch for scope `{}` -> {}. Reason: {}",
            scope,
            next,
            reason
        );
        Ok(next)
    }

    /// Verifies if a cached object's epoch is still valid relative to the active scope epoch.
    /// Returns true if it is safe to use.
    pub async fn is_valid(&self, scope: &str, object_epoch: u64) -> Result<bool> {
        let current = self.get_epoch(scope).await?;
        Ok(object_epoch >= current)
    }
}
