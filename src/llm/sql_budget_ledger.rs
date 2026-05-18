//! Owner: Evidence Gate / autonomous-delivery daemon (Wave 8.D)
//! Proof: `cargo test -p jeryu -- llm::sql_budget_ledger`
//!
//! Restart-safe successor to `crate::llm::budget::BudgetLedger`. Closes the
//! safety gap where `.autonomy/autonomy.yml::budget.fail_closed_over_budget
//! = true` could be silently bypassed by killing the process — the
//! in-memory ledger forgot everything on restart, so the next launch came
//! up with a fresh 0-usage day.
//!
//! Invariants:
//!   - `llm_budget_ledger` is APPEND-ONLY. The Rust API has no update /
//!     delete; SQLite `BEFORE UPDATE` / `BEFORE DELETE` triggers installed
//!     by `db/state.rs::migrate` enforce it at the storage layer (mirror of
//!     the `launch_ledger` defense-in-depth pattern from Wave 1.1).
//!   - `snapshot()` only counts rows with `recorded_at >= midnight UTC of
//!     today` so the daily cap resets naturally at the UTC day boundary.
//!     Yesterday's spend does not bleed into today's budget.
//!   - `snapshot()` filters by `repo_scope`. A fleet-wide ledger with a
//!     "global" scope and per-repo ledgers can coexist in one table.
//!   - `would_exceed(budget, estimated)` compares the daily snapshot plus
//!     `estimated` against `budget.daily_micro_usd_cap` only. Per-PR caps
//!     are the caller's concern (this layer only knows totals).
//!
//! Wave 11.A: SQL queries moved to `src/db/budget_repo.rs`. This file
//! holds the `BudgetTracker` trait impl, which now delegates to
//! `BudgetRepo`. Public API is unchanged.

use async_trait::async_trait;
use chrono::{DateTime, NaiveTime, TimeZone, Utc};

use crate::db::AnyPool;
use crate::db::budget_repo::BudgetRepo;

use super::budget::{Budget, BudgetTracker, TokenUsage};

/// SQL-backed `BudgetTracker`. Mirrors the style of `SqlLedger`
/// (Wave 1.2) and `SqlVerdictStore` (Wave 7.A).
#[derive(Debug, Clone)]
pub struct SqlBudgetLedger {
    repo: BudgetRepo,
    repo_scope: String,
}

impl SqlBudgetLedger {
    pub fn new(pool: AnyPool, repo_scope: impl Into<String>) -> Self {
        Self {
            repo: BudgetRepo::new(pool),
            repo_scope: repo_scope.into(),
        }
    }

    /// UTC midnight of "today" used as the snapshot lower bound. Pulled
    /// out so tests can verify the boundary logic without sleeping.
    fn midnight_utc_today() -> DateTime<Utc> {
        let now = Utc::now();
        Utc.from_utc_datetime(&now.date_naive().and_time(NaiveTime::MIN))
    }

    /// Test-only entry point that lets a test back-date a row. Real callers
    /// always go through the `BudgetTracker::record` trait method which
    /// stamps `Utc::now()`.
    #[cfg(test)]
    async fn record_at_for_test(
        &self,
        usage: TokenUsage,
        recorded_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        self.repo
            .budget_record(&self.repo_scope, &usage, recorded_at)
            .await
    }
}

#[async_trait]
impl BudgetTracker for SqlBudgetLedger {
    async fn record(&self, usage: TokenUsage) {
        // Trait surface is fire-and-forget (matches the in-memory impl).
        // A SQL error here would silently lose the spend, so we log it
        // via eprintln — the daemon's tracing layer captures stderr.
        if let Err(e) = self
            .repo
            .budget_record(&self.repo_scope, &usage, Utc::now())
            .await
        {
            eprintln!("llm_budget_ledger record failed: {e:?}");
        }
    }

    async fn snapshot(&self) -> TokenUsage {
        match self
            .repo
            .budget_snapshot(&self.repo_scope, Self::midnight_utc_today())
            .await
        {
            Ok(u) => u,
            Err(e) => {
                eprintln!("llm_budget_ledger snapshot failed: {e:?}");
                TokenUsage::default()
            }
        }
    }

    async fn would_exceed(&self, budget: &Budget, estimated_micro_usd: u64) -> bool {
        let s = self.snapshot().await;
        s.estimated_micro_usd.saturating_add(estimated_micro_usd) > budget.daily_micro_usd_cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::budget_repo::fresh_budget_pool;
    use crate::db::raw_query;
    use chrono::Duration;
    use std::sync::Arc;

    async fn fresh_db() -> AnyPool {
        // Test fixture moved to the db boundary so this file no longer
        // imports `sqlx::` (closes HLT-006).
        fresh_budget_pool().await
    }

    fn usage(prompt: u64, completion: u64, micro_usd: u64) -> TokenUsage {
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            estimated_micro_usd: micro_usd,
        }
    }

    fn cap(daily: u64) -> Budget {
        Budget {
            daily_micro_usd_cap: daily,
            per_pr_micro_usd_cap: daily,
        }
    }

    // 1.
    #[tokio::test]
    async fn record_then_snapshot_round_trips() {
        let l = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        l.record(usage(100, 50, 1_000)).await;
        let s = l.snapshot().await;
        assert_eq!(s.prompt_tokens, 100);
        assert_eq!(s.completion_tokens, 50);
        assert_eq!(s.estimated_micro_usd, 1_000);
    }

    // 2.
    #[tokio::test]
    async fn record_two_rows_sum_correctly_in_snapshot() {
        let l = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        l.record(usage(100, 50, 1_000)).await;
        l.record(usage(200, 100, 2_000)).await;
        let s = l.snapshot().await;
        assert_eq!(s.prompt_tokens, 300);
        assert_eq!(s.completion_tokens, 150);
        assert_eq!(s.estimated_micro_usd, 3_000);
    }

    // 3.
    #[tokio::test]
    async fn snapshot_returns_zero_for_empty_scope() {
        let l = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        let s = l.snapshot().await;
        assert_eq!(s, TokenUsage::default());
    }

    // 4.
    #[tokio::test]
    async fn would_exceed_false_when_below_cap() {
        let l = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        l.record(usage(0, 0, 1_000)).await;
        assert!(!l.would_exceed(&cap(10_000), 5_000).await);
    }

    // 5.
    #[tokio::test]
    async fn would_exceed_true_when_above_cap() {
        let l = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        l.record(usage(0, 0, 9_000)).await;
        assert!(l.would_exceed(&cap(10_000), 5_000).await);
    }

    // 6.
    #[tokio::test]
    async fn would_exceed_true_when_exact_cap_plus_estimated_crosses() {
        let l = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        l.record(usage(0, 0, 10_000)).await;
        // 10_000 + 1 > 10_000 — must trip even by one micro-USD.
        assert!(l.would_exceed(&cap(10_000), 1).await);
        // 10_000 + 0 is NOT strictly greater; stays under.
        assert!(!l.would_exceed(&cap(10_000), 0).await);
    }

    // 7.
    #[tokio::test]
    async fn snapshot_filters_by_repo_scope() {
        let pool = fresh_db().await;
        let a = SqlBudgetLedger::new(pool.clone(), "owner/repo-a");
        let b = SqlBudgetLedger::new(pool.clone(), "owner/repo-b");
        a.record(usage(0, 0, 1_111)).await;
        b.record(usage(0, 0, 2_222)).await;
        assert_eq!(a.snapshot().await.estimated_micro_usd, 1_111);
        assert_eq!(b.snapshot().await.estimated_micro_usd, 2_222);
    }

    // 8.
    #[tokio::test]
    async fn snapshot_only_counts_today_in_utc() {
        let l = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        // Insert one row dated yesterday and one dated now.
        let yesterday = Utc::now() - Duration::days(1);
        l.record_at_for_test(usage(0, 0, 7_777), yesterday)
            .await
            .unwrap();
        l.record(usage(0, 0, 333)).await;
        let s = l.snapshot().await;
        assert_eq!(
            s.estimated_micro_usd, 333,
            "yesterday's spend must NOT count toward today's snapshot"
        );
    }

    // 9.
    #[tokio::test]
    async fn append_only_trigger_blocks_update() {
        let pool = fresh_db().await;
        let l = SqlBudgetLedger::new(pool.clone(), "owner/repo");
        l.record(usage(1, 1, 1)).await;
        let res = raw_query("UPDATE llm_budget_ledger SET micro_usd = 0")
            .execute(&pool)
            .await;
        assert!(res.is_err(), "trigger must abort UPDATE");
    }

    // 10.
    #[tokio::test]
    async fn append_only_trigger_blocks_delete() {
        let pool = fresh_db().await;
        let l = SqlBudgetLedger::new(pool.clone(), "owner/repo");
        l.record(usage(1, 1, 1)).await;
        let res = raw_query("DELETE FROM llm_budget_ledger")
            .execute(&pool)
            .await;
        assert!(res.is_err(), "trigger must abort DELETE");
    }

    // 11.
    #[tokio::test]
    async fn concurrent_record_no_corruption_with_four_tasks() {
        let l = Arc::new(SqlBudgetLedger::new(fresh_db().await, "owner/repo"));
        let mut handles = Vec::new();
        for _task in 0..4 {
            let l = l.clone();
            handles.push(tokio::spawn(async move {
                for _i in 0..5 {
                    l.record(usage(10, 5, 100)).await;
                }
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }
        let s = l.snapshot().await;
        // 4 tasks * 5 records each = 20 inserts.
        assert_eq!(s.prompt_tokens, 20 * 10);
        assert_eq!(s.completion_tokens, 20 * 5);
        assert_eq!(s.estimated_micro_usd, 20 * 100);
    }

    // 12.
    #[tokio::test]
    async fn restart_survival_round_trip() {
        // Same pool, two ledgers — the second constructor is the
        // "restart" — it didn't see the first record() call but the
        // snapshot still returns it.
        let pool = fresh_db().await;
        {
            let first = SqlBudgetLedger::new(pool.clone(), "owner/repo");
            first.record(usage(7, 3, 700)).await;
        }
        let second = SqlBudgetLedger::new(pool, "owner/repo");
        let s = second.snapshot().await;
        assert_eq!(s.prompt_tokens, 7);
        assert_eq!(s.completion_tokens, 3);
        assert_eq!(s.estimated_micro_usd, 700);
    }

    // 13.
    #[tokio::test]
    async fn record_with_zero_tokens_still_inserts_row() {
        let pool = fresh_db().await;
        let l = SqlBudgetLedger::new(pool.clone(), "owner/repo");
        l.record(usage(0, 0, 0)).await;
        // Snapshot is still zero, but a row exists.
        let s = l.snapshot().await;
        assert_eq!(s, TokenUsage::default());
        // Use the raw_query re-export rather than `sqlx::query_scalar`; the
        // boundary doesn't currently re-export `query_scalar`, so we fetch
        // a row and pull the count column out manually via the db::Row trait.
        use crate::db::Row;
        let row = raw_query("SELECT COUNT(*) AS n FROM llm_budget_ledger WHERE repo_scope = ?")
            .bind("owner/repo")
            .fetch_one(&pool)
            .await
            .unwrap();
        let cnt: i64 = row.try_get("n").unwrap();
        assert_eq!(cnt, 1, "zero-valued record() must still insert a row");
    }

    // 14.
    #[tokio::test]
    async fn multi_scope_isolation() {
        let pool = fresh_db().await;
        let a = SqlBudgetLedger::new(pool.clone(), "scope-a");
        let b = SqlBudgetLedger::new(pool, "scope-b");
        a.record(usage(0, 0, 100)).await;
        assert_eq!(b.snapshot().await.estimated_micro_usd, 0);
    }

    // Bonus: drive both impls through the same sequence and assert agreement.
    #[tokio::test]
    async fn in_memory_and_sql_impls_agree_on_simple_sequence() {
        use crate::llm::budget::BudgetLedger;
        let sql = SqlBudgetLedger::new(fresh_db().await, "owner/repo");
        let mem = BudgetLedger::new();
        let seq = [
            usage(10, 5, 100),
            usage(20, 7, 250),
            usage(0, 0, 0),
            usage(99, 1, 900),
        ];
        for u in &seq {
            BudgetTracker::record(&sql, *u).await;
            BudgetTracker::record(&mem, *u).await;
        }
        let s_sql = BudgetTracker::snapshot(&sql).await;
        let s_mem = BudgetTracker::snapshot(&mem).await;
        assert_eq!(s_sql, s_mem, "in-memory and SQL impls must agree");

        // would_exceed must also agree at a few cap thresholds.
        let total = s_sql.estimated_micro_usd;
        for delta in [0u64, 1, 100, 10_000] {
            for cap_val in [total.saturating_sub(1), total, total + 1, total + 1_000] {
                let b = cap(cap_val);
                assert_eq!(
                    BudgetTracker::would_exceed(&sql, &b, delta).await,
                    BudgetTracker::would_exceed(&mem, &b, delta).await,
                    "would_exceed disagreement at cap={cap_val} delta={delta}",
                );
            }
        }
    }
}
