//! Owner: Release Pipeline (composite gate)
//! Proof: `cargo test -p jeryu -- release::gate`
//! Invariants: Composite gate is the single source of truth for jeryu/release-ready.
//!
//! Implements the composite `jeryu/release-ready` gate described in
//! `release.policy.toml` and `docs/release-policy.md`. The gate is itself a
//! pure data structure (`ReleaseReadyGate`) with one `Receipt` per required
//! component. Non-dry-run composition loads required receipts from the
//! repo-local evidence directory and fails closed when they are absent.
//! The CLI calls `compose_gate` then optionally posts to GitHub.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[path = "gate_logic.rs"]
mod gate_logic;
pub use gate_logic::{compose_gate, post_check_run, render_gate_text};

/// One receipt feeding the composite gate. Identifier matches
/// `release.policy.toml [gate.jeryu_release_ready] required_receipts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub id: String,
    pub status: ReceiptStatus,
    pub detail: String,
    /// Optional path to the evidence artifact (e.g. capsule.json).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Pass,
    Fail,
    Skipped,
    Pending,
}

impl ReceiptStatus {
    pub fn is_blocking(&self) -> bool {
        matches!(self, Self::Fail | Self::Pending)
    }
}

/// Composite gate composed from required receipts. Pass iff every required
/// receipt is `Pass` or `Skipped`-with-justification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReadyGate {
    pub pr: u64,
    pub overall: ReceiptStatus,
    pub receipts: Vec<Receipt>,
    pub summary: String,
}

impl ReleaseReadyGate {
    pub fn is_pass(&self) -> bool {
        self.overall == ReceiptStatus::Pass
    }
}

/// The canonical receipt ids required by the composite gate.
/// Must stay in sync with `release.policy.toml [gate.jeryu_release_ready]`.
pub const REQUIRED_RECEIPTS: &[&str] = &[
    "intake",
    "vti-plan",
    "proof-receipt",
    "risk-gate",
    "reviewer-agent",
    "rollback-plan",
    "ci-checks",
];

const DEFAULT_RECEIPT_DIR: &str = ".jeryu/release-ready/receipts";

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
