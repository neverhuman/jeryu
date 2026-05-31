//! Multi-reviewer orchestrator.
//!
//! Runs the required reviewer agents (Security / TestIntegrity / Runtime /
//! Lockfile) concurrently against a single `EvidencePack`, gated by the
//! `BudgetLedger`, and returns one signed `AgentApprovalReceipt` per role.
//!
//! Invariants:
//!   - One reviewer failing (LLM error, parse error, budget exhausted) NEVER
//!     aborts the whole batch — it becomes an `Abstain` receipt instead.
//!   - Every synthesized receipt carries the input pack's `evidence_pack_id`,
//!     `head_sha`, and `policy_sha`, so the judge's SHA-binding doesn't drop
//!     them later.
//!   - Every synthesized receipt has `not_author: true`.
//!   - Synthesized abstain receipts are signed with the orchestrator's ed25519
//!     key so the judge's `evidence_signature_invalid` condition accepts them.

mod fake;
mod production;
mod receipt;

pub use fake::FakeReviewerOrchestrator;
pub use production::ProductionReviewerOrchestrator;

use crate::schema::{AgentApprovalReceipt, EvidencePack, ReviewerRole};
use anyhow::Result;
use async_trait::async_trait;

/// Estimated micro-USD cost of one reviewer call. Used to decide whether the
/// next call would exceed the daily cap; actual usage is recorded after.
pub const ESTIMATED_REVIEWER_COST_MICRO_USD: u64 = 5_000;

#[async_trait]
pub trait ReviewerOrchestrator: Send + Sync {
    /// Run every reviewer whose role is in `required_roles`. Return one receipt
    /// per role attempted. If a single role fails it produces an `Abstain`
    /// receipt; it does NOT abort the batch.
    async fn run_all(
        &self,
        pack: &EvidencePack,
        required_roles: &[ReviewerRole],
        diff_text: &str,
    ) -> Result<Vec<AgentApprovalReceipt>>;
}
