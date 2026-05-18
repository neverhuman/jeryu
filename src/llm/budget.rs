//! Per-repo + per-PR token / cost ledger.
//!
//! Phase 0 implementation: in-memory only. Phase 0.5 (Wave 8.D) introduces
//! `crate::llm::sql_budget_ledger::SqlBudgetLedger`, a SQL-backed
//! `BudgetTracker` that survives process restart. Both impls satisfy the
//! `BudgetTracker` trait so call sites can swap them without code change.
//!
//! Why the dual surface (sync `BudgetLedger` + async `BudgetTracker`)?
//! `ProductionReviewerOrchestrator` (Wave 8.B) and a handful of tests call
//! the in-memory ledger's sync methods directly. Changing them all to
//! `.await` would ripple far beyond this wave. Instead we keep the sync API
//! intact and ADD an async trait whose in-memory impl delegates to the sync
//! methods — sites that adopt SQL move to `Arc<dyn BudgetTracker>` and call
//! the trait, sites that stay in-memory keep using the sync methods.

use std::sync::Mutex;

use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct Budget {
    pub daily_micro_usd_cap: u64,
    pub per_pr_micro_usd_cap: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub estimated_micro_usd: u64,
}

#[derive(Debug, Default)]
pub struct BudgetLedger {
    pub total_today: Mutex<TokenUsage>,
}

impl BudgetLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, u: TokenUsage) {
        let mut t = self.total_today.lock().unwrap();
        t.prompt_tokens += u.prompt_tokens;
        t.completion_tokens += u.completion_tokens;
        t.estimated_micro_usd += u.estimated_micro_usd;
    }

    pub fn snapshot(&self) -> TokenUsage {
        *self.total_today.lock().unwrap()
    }

    /// Returns true if the next call (estimated) would exceed budget.
    pub fn would_exceed(&self, budget: &Budget, estimated_micro_usd: u64) -> bool {
        let s = self.snapshot();
        s.estimated_micro_usd + estimated_micro_usd > budget.daily_micro_usd_cap
    }
}

/// Pluggable token / cost ledger surface. Wave 8.D introduced this so the
/// in-memory `BudgetLedger` and the SQL-backed `SqlBudgetLedger` can sit
/// behind the same `Arc<dyn BudgetTracker>` in callers that need restart
/// survival.
///
/// Why async even though the in-memory impl is sync? The persistent
/// impl awaits the db boundary — and a trait split would defeat the
/// purpose. The in-memory impl simply delegates to the existing sync
/// methods.
#[async_trait]
pub trait BudgetTracker: Send + Sync {
    async fn record(&self, usage: TokenUsage);
    async fn snapshot(&self) -> TokenUsage;
    async fn would_exceed(&self, budget: &Budget, estimated_micro_usd: u64) -> bool;
}

#[async_trait]
impl BudgetTracker for BudgetLedger {
    async fn record(&self, usage: TokenUsage) {
        // Delegate to the sync method. The mutex is held only for the
        // duration of three integer adds, so blocking from inside an async
        // context is acceptable.
        BudgetLedger::record(self, usage);
    }

    async fn snapshot(&self) -> TokenUsage {
        BudgetLedger::snapshot(self)
    }

    async fn would_exceed(&self, budget: &Budget, estimated_micro_usd: u64) -> bool {
        BudgetLedger::would_exceed(self, budget, estimated_micro_usd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_records_and_caps() {
        let l = BudgetLedger::new();
        l.record(TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            estimated_micro_usd: 1_000,
        });
        l.record(TokenUsage {
            prompt_tokens: 200,
            completion_tokens: 100,
            estimated_micro_usd: 2_000,
        });
        let s = l.snapshot();
        assert_eq!(s.prompt_tokens, 300);
        assert_eq!(s.completion_tokens, 150);
        assert_eq!(s.estimated_micro_usd, 3_000);
        let b = Budget {
            daily_micro_usd_cap: 5_000,
            per_pr_micro_usd_cap: 1_000,
        };
        assert!(!l.would_exceed(&b, 1_000));
        assert!(l.would_exceed(&b, 3_000));
    }

    /// The `BudgetTracker` async impl on `BudgetLedger` must observe the
    /// same state as the sync API — they share one `Mutex<TokenUsage>`.
    #[tokio::test]
    async fn in_memory_tracker_impl_delegates_to_sync_state() {
        let l = BudgetLedger::new();
        // Sync record, async snapshot.
        l.record(TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            estimated_micro_usd: 200,
        });
        let s = BudgetTracker::snapshot(&l).await;
        assert_eq!(s.prompt_tokens, 10);
        assert_eq!(s.estimated_micro_usd, 200);

        // Async record, sync snapshot.
        BudgetTracker::record(
            &l,
            TokenUsage {
                prompt_tokens: 1,
                completion_tokens: 2,
                estimated_micro_usd: 50,
            },
        )
        .await;
        let s2 = l.snapshot();
        assert_eq!(s2.prompt_tokens, 11);
        assert_eq!(s2.estimated_micro_usd, 250);

        let b = Budget {
            daily_micro_usd_cap: 1_000,
            per_pr_micro_usd_cap: 100,
        };
        assert!(!BudgetTracker::would_exceed(&l, &b, 700).await);
        assert!(BudgetTracker::would_exceed(&l, &b, 800).await);
    }
}
