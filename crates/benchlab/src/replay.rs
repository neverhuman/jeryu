//! Benchmark replay verification.

use crate::receipt::{BenchmarkReceipt, ReceiptError};
use std::collections::BTreeMap;

/// Replay plan derived from receipts.
#[derive(Clone, Debug, Default)]
pub struct ReplayPlan {
    receipts: BTreeMap<String, BenchmarkReceipt>,
}

impl ReplayPlan {
    /// Create an empty plan.
    pub fn new() -> Self {
        Self {
            receipts: BTreeMap::new(),
        }
    }

    /// Add a receipt to the replay plan.
    pub fn add_receipt(&mut self, receipt: BenchmarkReceipt) -> Result<(), ReceiptError> {
        receipt.validate()?;
        self.receipts.insert(receipt.benchmark_id.clone(), receipt);
        Ok(())
    }

    /// Number of receipts in the plan.
    pub fn len(&self) -> usize {
        self.receipts.len()
    }

    /// True when no receipts are present.
    pub fn is_empty(&self) -> bool {
        self.receipts.is_empty()
    }

    /// Verify all receipts are replayable and cache safe.
    pub fn verify(&self) -> ReplayVerdict {
        let mut failures = Vec::new();
        for receipt in self.receipts.values() {
            if let Err(error) = receipt.validate() {
                failures.push(format!("{}: {error:?}", receipt.benchmark_id));
            }
            if !receipt.cache_safe() {
                failures.push(format!("{}: false cache hit", receipt.benchmark_id));
            }
            if !receipt.reproduce.ends_with(&receipt.benchmark_id) {
                failures.push(format!("{}: replay command mismatch", receipt.benchmark_id));
            }
        }
        ReplayVerdict {
            checked: self.receipts.len(),
            failures,
        }
    }
}

/// Result of replay verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayVerdict {
    pub checked: usize,
    pub failures: Vec<String>,
}

impl ReplayVerdict {
    /// True when every receipt is valid and replayable.
    pub fn passed(&self) -> bool {
        self.checked > 0 && self.failures.is_empty()
    }
}
