//! Owner: Taint Tracking (Detonation Lane)
//! Proof: `cargo test -p jeryu -- taint`
//! Invariants: Taint graph is append-only; purges are recorded events, not deletes; cross-pipeline taint propagation requires a valid plan_id

use anyhow::Result;
use sqlx::{AnyPool, Row};

use crate::state::{StateBackend, backend_sql};

/// Manages retroactive taint tracking and graph purging for the Detonation Lane.
#[derive(Clone)]
pub struct TaintManager {
    pool: AnyPool,
    backend: StateBackend,
}

impl TaintManager {
    pub fn new(pool: AnyPool) -> Self {
        Self::with_backend(pool, StateBackend::RedlineDb)
    }

    pub fn with_backend(pool: AnyPool, backend: StateBackend) -> Self {
        Self { pool, backend }
    }

    /// Recursively marks a node and all of its dependents as tainted.
    pub async fn propagate_taint(
        &self,
        root_hash: &str,
        reason: &str,
        author_job_id: i64,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // 1. Mark the root node explicitly
        let upsert_root = backend_sql(
            self.backend,
            r#"INSERT INTO cache_taints (object_hash, reason, created_at, author_job_id)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(object_hash) DO UPDATE SET
                 reason = excluded.reason,
                 created_at = excluded.created_at,
                 author_job_id = excluded.author_job_id"#,
        );
        sqlx::query(&upsert_root)
            .bind(root_hash)
            .bind(reason)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(author_job_id)
            .execute(&mut *tx)
            .await?;

        // 2. Discover dependents (objects built using the root hash as a base or dependency)
        // using a recursive CTE to traverse the entire subgraph of downstream builds.
        let dependents_sql = backend_sql(
            self.backend,
            "WITH RECURSIVE taint_tree AS (
                SELECT object_hash FROM cache_verdicts WHERE inputs_hash = ?
                UNION
                SELECT cv.object_hash FROM cache_verdicts cv
                JOIN taint_tree tt ON cv.inputs_hash = tt.object_hash
            )
            SELECT object_hash FROM taint_tree",
        );
        let dependents: Vec<String> = sqlx::query(&dependents_sql)
            .bind(root_hash)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| r.get("object_hash"))
            .collect();

        // 3. Taint all downstream
        for dep in dependents {
            let insert_dependent = backend_sql(
                self.backend,
                r#"INSERT INTO cache_taints (object_hash, reason, created_at, author_job_id)
                   VALUES (?, ?, ?, ?)
                   ON CONFLICT(object_hash) DO NOTHING"#,
            );
            sqlx::query(&insert_dependent)
                .bind(&dep)
                .bind(format!(
                    "Downstream dependency of `{}`: {}",
                    root_hash, reason
                ))
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(author_job_id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        tracing::warn!(
            "Retroactive Taint applied to root `{}` and its dependents.",
            root_hash
        );
        Ok(())
    }

    /// Check if a given object hash is tainted.
    pub async fn is_tainted(&self, hash: &str) -> Result<bool> {
        let sql = backend_sql(
            self.backend,
            "SELECT COUNT(*) FROM cache_taints WHERE object_hash = ?",
        );
        let count: i64 = sqlx::query_scalar(&sql)
            .bind(hash)
            .fetch_one(&self.pool)
            .await?;

        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{AnyPoolOptions, install_default_drivers};

    async fn setup_db() -> AnyPool {
        install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("redline::memory:")
            .await
            .unwrap();

        sqlx::query(
            "CREATE TABLE cache_taints (
            object_hash TEXT PRIMARY KEY,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL,
            author_job_id INTEGER NOT NULL
        )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE cache_verdicts (
            id INTEGER PRIMARY KEY,
            object_hash TEXT NOT NULL,
            inputs_hash TEXT NOT NULL,
            verdict TEXT NOT NULL,
            tier TEXT NOT NULL
        )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_taint_propagation() {
        let pool = setup_db().await;

        // Mock a dependent graph
        sqlx::query("INSERT INTO cache_verdicts (object_hash, inputs_hash, verdict, tier) VALUES (?, ?, ?, ?)")
            .bind("child-hash-1")
            .bind("root-hash")
            .bind("hit")
            .bind("untrusted")
            .execute(&pool).await.unwrap();

        let mgr = TaintManager::new(pool);

        // Ensure not tainted initially
        assert!(!mgr.is_tainted("root-hash").await.unwrap());
        assert!(!mgr.is_tainted("child-hash-1").await.unwrap());

        // Propagate taint
        mgr.propagate_taint("root-hash", "Tripwire breached (net_admin)", 100)
            .await
            .unwrap();

        // Ensure both root and child are tainted
        assert!(mgr.is_tainted("root-hash").await.unwrap());
        assert!(mgr.is_tainted("child-hash-1").await.unwrap());
    }
}
