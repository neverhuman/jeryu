//! Unit tests for the in-memory verdict store, grouped by responsibility:
//!   - [`save`]: save/load_latest round-trips, idempotency, supersede-on-save.
//!   - [`query`]: list_active filtering/ordering, explicit supersede, concurrency.

mod query;
mod save;

use super::*;
use crate::signing::Signature;
use crate::types::{RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict};

/// Shared fixture: mint a fully-populated verdict with a deterministic id
/// derived from the creation timestamp and head-SHA tail.
fn mint_verdict(
    repo: &str,
    pr: Option<&str>,
    head_sha_tail: &str,
    decision: GateDecision,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> VibeGateVerdict {
    let head_sha = format!("{head_sha_tail:0>40}");
    let id = format!(
        "vgv_{}_{}",
        created_at.timestamp_millis(),
        &head_sha[head_sha.len().saturating_sub(8)..]
    );
    VibeGateVerdict {
        schema: SchemaTag::new(),
        id,
        evidence_pack_id: "ep_test".into(),
        pull_request: pr.map(|s| s.to_string()),
        repo: repo.into(),
        target_branch: "main".into(),
        head_sha,
        policy_sha: "c".repeat(40),
        evidence_pack_digest: "sha256:deadbeef".into(),
        risk: RiskTier::R2,
        hard_stops: vec![],
        required_reviews: vec![],
        approval_receipts: Vec::<VerdictReceiptRef>::new(),
        decision,
        valid_for_head_sha_only: true,
        rebind_on_train: true,
        expires_at,
        created_at,
        signature: Signature::unsigned(),
    }
}
