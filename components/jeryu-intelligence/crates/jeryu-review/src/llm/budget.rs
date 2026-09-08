//! Per-repo + per-PR token / cost ledger (in-memory).

use std::sync::Mutex;

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

    /// Returns true if the next call (estimated) would exceed the daily cap.
    pub fn would_exceed(&self, budget: &Budget, estimated_micro_usd: u64) -> bool {
        let s = self.snapshot();
        s.estimated_micro_usd + estimated_micro_usd > budget.daily_micro_usd_cap
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
        assert_eq!(s.estimated_micro_usd, 3_000);
        let b = Budget {
            daily_micro_usd_cap: 5_000,
            per_pr_micro_usd_cap: 1_000,
        };
        assert!(!l.would_exceed(&b, 1_000));
        assert!(l.would_exceed(&b, 3_000));
    }
}
