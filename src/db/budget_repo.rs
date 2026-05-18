//! Owner: db-boundary (Wave 11.A)
//! Proof: `cargo test -p jeryu --lib -- db::budget_repo`
//!
//! Typed repo for `llm_budget_ledger`. Closes
//! `HLT-006-DIRECT-DB-WRONG-LAYER` for the LLM budget side: the
//! `SqlBudgetLedger` wrapper no longer imports `sqlx::`.
//!
//! Invariants preserved (mirror of `SqlBudgetLedger`):
//!   - `llm_budget_ledger` is APPEND-ONLY. The repo has no
//!     update/delete; SQLite triggers enforce it.
//!   - `budget_snapshot` only counts rows with `recorded_at >= since`
//!     so the caller decides the day-boundary cutoff.
//!   - `budget_snapshot` filters by `scope` so a fleet-wide ledger
//!     can coexist with per-repo ledgers in one table.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::AnyPool;
use sqlx::Row;

use crate::llm::budget::TokenUsage;

/// Typed repo for the `llm_budget_ledger` table.
#[derive(Debug, Clone)]
pub struct BudgetRepo {
    pool: AnyPool,
}

impl BudgetRepo {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// Append one row to the budget ledger.
    pub async fn budget_record(
        &self,
        scope: &str,
        usage: &TokenUsage,
        recorded_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO llm_budget_ledger
                 (repo_scope, prompt_tokens, completion_tokens, micro_usd, recorded_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(scope)
        .bind(usage.prompt_tokens as i64)
        .bind(usage.completion_tokens as i64)
        .bind(usage.estimated_micro_usd as i64)
        .bind(recorded_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("insert llm_budget_ledger row")?;
        Ok(())
    }

    /// Sum every row matching `scope` AND `recorded_at >= since`. Returns
    /// `TokenUsage::default()` when no rows match.
    pub async fn budget_snapshot(&self, scope: &str, since: DateTime<Utc>) -> Result<TokenUsage> {
        let row = sqlx::query(
            "SELECT
                 COALESCE(SUM(prompt_tokens),     0) AS p,
                 COALESCE(SUM(completion_tokens), 0) AS c,
                 COALESCE(SUM(micro_usd),         0) AS u
             FROM llm_budget_ledger
             WHERE repo_scope = ? AND recorded_at >= ?",
        )
        .bind(scope)
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .context("sum llm_budget_ledger for snapshot")?;
        let p: i64 = row.try_get("p").unwrap_or(0);
        let c: i64 = row.try_get("c").unwrap_or(0);
        let u: i64 = row.try_get("u").unwrap_or(0);
        Ok(TokenUsage {
            prompt_tokens: p.max(0) as u64,
            completion_tokens: c.max(0) as u64,
            estimated_micro_usd: u.max(0) as u64,
        })
    }
}

// ---------------------------------------------------------------------------
// In-memory test schema installer.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) async fn fresh_budget_pool() -> AnyPool {
    use sqlx::any::{AnyPoolOptions, install_default_drivers};
    install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect in-memory sqlite");
    for stmt in budget_schema_ddl() {
        sqlx::query(stmt).execute(&pool).await.unwrap();
    }
    pool
}

#[cfg(test)]
pub(crate) fn budget_schema_ddl() -> &'static [&'static str] {
    &[
        "CREATE TABLE llm_budget_ledger (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_scope TEXT NOT NULL,
            prompt_tokens INTEGER NOT NULL,
            completion_tokens INTEGER NOT NULL,
            micro_usd INTEGER NOT NULL,
            recorded_at TEXT NOT NULL)",
        "CREATE INDEX idx_llm_budget_ledger_scope_time
            ON llm_budget_ledger(repo_scope, recorded_at DESC)",
        "CREATE TRIGGER llm_budget_ledger_no_update
             BEFORE UPDATE ON llm_budget_ledger
         BEGIN SELECT RAISE(ABORT, 'llm_budget_ledger is append-only'); END",
        "CREATE TRIGGER llm_budget_ledger_no_delete
             BEFORE DELETE ON llm_budget_ledger
         BEGIN SELECT RAISE(ABORT, 'llm_budget_ledger is append-only'); END",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn usage(p: u64, c: u64, u: u64) -> TokenUsage {
        TokenUsage {
            prompt_tokens: p,
            completion_tokens: c,
            estimated_micro_usd: u,
        }
    }

    // -- 1.
    #[tokio::test]
    async fn budget_record_then_snapshot_roundtrips() {
        let repo = BudgetRepo::new(fresh_budget_pool().await);
        let now = Utc::now();
        repo.budget_record("owner/repo", &usage(100, 50, 1_000), now)
            .await
            .unwrap();
        let s = repo
            .budget_snapshot("owner/repo", now - Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(s.prompt_tokens, 100);
        assert_eq!(s.completion_tokens, 50);
        assert_eq!(s.estimated_micro_usd, 1_000);
    }

    // -- 2.
    #[tokio::test]
    async fn budget_snapshot_filters_by_scope() {
        let pool = fresh_budget_pool().await;
        let repo = BudgetRepo::new(pool);
        let now = Utc::now();
        repo.budget_record("a", &usage(0, 0, 1_111), now)
            .await
            .unwrap();
        repo.budget_record("b", &usage(0, 0, 2_222), now)
            .await
            .unwrap();
        let s_a = repo
            .budget_snapshot("a", now - Duration::seconds(1))
            .await
            .unwrap();
        let s_b = repo
            .budget_snapshot("b", now - Duration::seconds(1))
            .await
            .unwrap();
        assert_eq!(s_a.estimated_micro_usd, 1_111);
        assert_eq!(s_b.estimated_micro_usd, 2_222);
    }

    // -- 3.
    #[tokio::test]
    async fn budget_snapshot_filters_by_time() {
        let repo = BudgetRepo::new(fresh_budget_pool().await);
        let now = Utc::now();
        repo.budget_record("owner/repo", &usage(0, 0, 7_777), now - Duration::days(1))
            .await
            .unwrap();
        repo.budget_record("owner/repo", &usage(0, 0, 333), now)
            .await
            .unwrap();
        let today = repo
            .budget_snapshot("owner/repo", now - Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(
            today.estimated_micro_usd, 333,
            "yesterday's spend must not count toward today's snapshot"
        );
    }

    // -- 4.
    #[tokio::test]
    async fn budget_snapshot_empty_returns_default() {
        let repo = BudgetRepo::new(fresh_budget_pool().await);
        let s = repo.budget_snapshot("nobody", Utc::now()).await.unwrap();
        assert_eq!(s, TokenUsage::default());
    }
}
