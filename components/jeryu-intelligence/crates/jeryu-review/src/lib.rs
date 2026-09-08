//! Owner: Autonomy gate — multi-reviewer orchestrator + approval fusion
//!        (standalone, self-contained crate).
//! Proof: `cargo build && cargo test` inside this crate's directory.
//!
//! Cross-cutting invariants:
//!   - The diff is always wrapped as untrusted input (`<diff>…</diff>`); the
//!     reviewer prompts cannot be overridden by diff content.
//!   - Strict-schema parse → on failure the reviewer ABSTAINS (never guesses).
//!   - The judge NEVER reads code — it fuses signed receipts + policy only.
//!   - Every receipt records `prompt_sha` / `model` / `provider` /
//!     `raw_response_sha`, and is SHA-bound to its pack's
//!     `(evidence_pack_id, head_sha, policy_sha)` (Law 4).
#![forbid(unsafe_code)]
//!   - Veto > approval: any single `Block` or hard-stop → Reject.
//!   - Fail-closed: missing quorum policy, unsigned pack, secret-in-diff,
//!     exhausted budget, or a missing prompt → Abstain / RequireHuman / Reject,
//!     never AllowMerge.
//!
//! This crate is SELF-CONTAINED. Thin trait seams (`LlmProvider`, `Reviewer`)
//! stand in for the external systems (LLM client, reviewer engine); the wire
//! schema, signing primitives, conditions registry, and policy bundle are
//! hosted here until the autonomy/proof port absorbs them. A real impl wires
//! the forge PR / proof receipts / store later; deterministic in-memory
//! implementations ship for tests.

pub mod approval;
pub mod conditions;
pub mod judge;
pub mod llm;
pub mod orchestrator;
pub mod parse;
pub mod policy;
pub mod prompt_builder;
pub mod rejudge;
pub mod reviewers;
pub mod schema;
pub mod signing;

#[cfg(test)]
mod test_support;

// --- Curated re-exports (the crate's public seam) --------------------------

pub use approval::{
    QuorumDecision, QuorumOutcome, ShaBindError, evaluate_quorum, verify_sha_binding,
};
pub use conditions::{ConditionRegistry, HardStop};
pub use judge::{JudgeInputs, JudgeOutcome, judge, mint_verdict_id};
pub use orchestrator::{
    ESTIMATED_REVIEWER_COST_MICRO_USD, FakeReviewerOrchestrator, ProductionReviewerOrchestrator,
    ReviewerOrchestrator,
};
pub use parse::{ParsedReceiptFields, extract_receipt_json};
pub use policy::{ApprovalRules, ApprovalsPolicy, HardStopEntry, PolicyBundle, QuorumEntry};
pub use prompt_builder::{ReviewerPromptInputs, build_reviewer_messages, prompt_sha};
pub use rejudge::{LiveState, RejudgeReason, check, must_rejudge};
pub use reviewers::{
    ReviewContext, ReviewInputs, Reviewer, ReviewerCallError, ReviewerRoleId, run_review,
};
pub use schema::{
    AgentApprovalReceipt, EvidencePack, Finding, GateDecision, ReviewDecision, ReviewerRole,
    RiskTier, Severity, TokenCounts, VerdictReceiptRef, VibeGateVerdict,
};
pub use signing::{EdSigningKey, EdVerifier, Signature, sha256_digest};
