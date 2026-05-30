//! In-memory AgentBridge state store.

use forge_core::phase7::{PullRequest, PullRequestId, Receipt, ReceiptId};
use std::collections::BTreeMap;

/// In-memory state. Production would back this with Postgres and an event log.
#[derive(Clone, Debug, Default)]
pub struct AgentBridgeState {
    prs: BTreeMap<PullRequestId, PullRequest>,
    receipts: BTreeMap<ReceiptId, Receipt>,
}

impl AgentBridgeState {
    /// Creates an empty state store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a PR.
    pub fn upsert_pr(&mut self, pr: PullRequest) {
        self.prs.insert(pr.id.clone(), pr);
    }

    /// Reads a PR.
    pub fn pr(&self, id: &PullRequestId) -> Option<&PullRequest> {
        self.prs.get(id)
    }

    /// Adds a receipt.
    pub fn add_receipt(&mut self, receipt: Receipt) {
        self.receipts.insert(receipt.id.clone(), receipt);
    }

    /// Reads a receipt.
    pub fn receipt(&self, id: &ReceiptId) -> Option<&Receipt> {
        self.receipts.get(id)
    }
}
