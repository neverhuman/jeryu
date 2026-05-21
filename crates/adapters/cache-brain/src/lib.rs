//! Owner: Cache Brain DB Adapter (sqlx-backed action_cache lookup)
//! Proof: `cargo test -p cache-brain-adapter`
//! Invariants: All SQL/sqlx access for the action_cache table lives here; callers see only the trait.
//!
//! This crate exists to satisfy the architectural rule that direct database
//! access (sqlx + raw SQL) must live under `crates/adapters/` (or `db/`),
//! not in the application layer (`src/`).

use anyhow::Result;
use async_trait::async_trait;

/// Re-export of `sqlx::AnyPool` so the application layer never has to name `sqlx`
/// directly when only a pool handle is required.
pub use sqlx::AnyPool;

/// Backend kind for SQL execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterBackend {
    Sqlite,
    RedlineDb,
}

/// A row in the `action_cache` table mapped to its trust namespace.
#[derive(Debug, Clone)]
pub struct ActionCacheEntry {
    pub namespace: String,
}

/// Adapter trait the application uses to query the action_cache.
///
/// Implementations are responsible for the actual SQL.
#[async_trait]
pub trait ActionCacheStore: Send + Sync {
    /// Look up an action_cache row by its action key (input signature).
    async fn lookup(&self, action_key: &str) -> Result<Option<ActionCacheEntry>>;
}

/// sqlx-backed implementation of `ActionCacheStore`.
///
/// Holds an `sqlx::AnyPool` and emits the canonical
/// `SELECT namespace, created_at FROM action_cache WHERE action_key = ?` query.
pub struct SqlxActionCacheStore {
    pool: sqlx::AnyPool,
    backend: AdapterBackend,
}

impl SqlxActionCacheStore {
    pub fn new(pool: sqlx::AnyPool, backend: AdapterBackend) -> Self {
        Self { pool, backend }
    }

    /// Construct as a trait object suitable for handing to the application layer.
    pub fn boxed(
        pool: sqlx::AnyPool,
        backend: AdapterBackend,
    ) -> std::sync::Arc<dyn ActionCacheStore> {
        std::sync::Arc::new(Self::new(pool, backend))
    }
}

/// Create an action-cache store backed by the configured pool.
///
/// Callers in `src/` should use this factory instead of naming the
/// concrete implementation type directly, keeping the `sqlx` token
/// confined to `crates/adapters/`.
pub fn create_action_store(
    pool: sqlx::AnyPool,
    backend: AdapterBackend,
) -> std::sync::Arc<dyn ActionCacheStore> {
    SqlxActionCacheStore::boxed(pool, backend)
}

#[async_trait]
impl ActionCacheStore for SqlxActionCacheStore {
    async fn lookup(&self, action_key: &str) -> Result<Option<ActionCacheEntry>> {
        let _backend = self.backend;
        let sql = "SELECT namespace, created_at FROM action_cache WHERE action_key = ?";
        let row: Option<(String, String)> = sqlx::query_as(sql)
            .bind(action_key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(namespace, _)| ActionCacheEntry { namespace }))
    }
}

/// Count active rows in the `cache_taints` table.
///
/// Lives in this adapter so the application layer never issues raw SQL.
/// The query takes no bind parameters, so dialect rewriting is not required.
pub async fn count_active_cache_taints(pool: &sqlx::AnyPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cache_taints")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Look up the current epoch for a given scope. Returns `0` when the row is missing.
///
/// Owns the `SELECT current_epoch FROM cache_epochs WHERE scope = ?` query.
pub async fn current_epoch_for(
    pool: &sqlx::AnyPool,
    backend: AdapterBackend,
    scope: &str,
) -> Result<i64> {
    let _backend = backend;
    let sql = "SELECT current_epoch FROM cache_epochs WHERE scope = ?";
    let epoch: i64 = sqlx::query_scalar(sql)
        .bind(scope)
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);
    Ok(epoch)
}

/// Persist the next epoch for a scope along with audit fields. Owns the
/// `INSERT ... ON CONFLICT(scope) DO UPDATE` query.
pub async fn upsert_epoch_for(
    pool: &sqlx::AnyPool,
    backend: AdapterBackend,
    scope: &str,
    next_epoch: i64,
    updated_at: &str,
    author_job_id: i64,
    reason: &str,
) -> Result<()> {
    let _backend = backend;
    let sql = r#"INSERT INTO cache_epochs (scope, current_epoch, updated_at, author_job_id, reason)
           VALUES (?, ?, ?, ?, ?)
           ON CONFLICT(scope) DO UPDATE SET
             current_epoch = excluded.current_epoch,
             updated_at = excluded.updated_at,
             author_job_id = excluded.author_job_id,
             reason = excluded.reason"#;
    sqlx::query(sql)
        .bind(scope)
        .bind(next_epoch)
        .bind(updated_at)
        .bind(author_job_id)
        .bind(reason)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod epoch_tests {
    use super::*;
    use redlinedb_sqlx::install_default_drivers;
    use sqlx::any::AnyPoolOptions;
    use tempfile::NamedTempFile;

    async fn setup_pool() -> sqlx::AnyPool {
        install_default_drivers();
        let tmp = NamedTempFile::new().expect("tempfile for cache-brain pool");
        let url = format!("redline:{}?mode=rwc", tmp.path().display());
        let pool = AnyPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .unwrap();
        std::mem::forget(tmp);
        sqlx::query(
            "CREATE TABLE cache_epochs (
                scope TEXT PRIMARY KEY,
                current_epoch INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                author_job_id INTEGER NOT NULL,
                reason TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn current_epoch_defaults_to_zero() {
        let pool = setup_pool().await;
        let v = current_epoch_for(&pool, AdapterBackend::RedlineDb, "scope:x")
            .await
            .unwrap();
        assert_eq!(v, 0);
    }

    #[tokio::test]
    async fn upsert_then_read_roundtrip() {
        let pool = setup_pool().await;
        upsert_epoch_for(
            &pool,
            AdapterBackend::RedlineDb,
            "scope:x",
            7,
            "2026-01-01T00:00:00Z",
            42,
            "test",
        )
        .await
        .unwrap();
        let v = current_epoch_for(&pool, AdapterBackend::RedlineDb, "scope:x")
            .await
            .unwrap();
        assert_eq!(v, 7);

        // ON CONFLICT branch
        upsert_epoch_for(
            &pool,
            AdapterBackend::RedlineDb,
            "scope:x",
            8,
            "2026-01-01T00:00:01Z",
            43,
            "bump",
        )
        .await
        .unwrap();
        let v = current_epoch_for(&pool, AdapterBackend::RedlineDb, "scope:x")
            .await
            .unwrap();
        assert_eq!(v, 8);
    }
}
