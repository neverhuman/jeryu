//! Owner: Evidence Gate / approval & quorum
//! Proof: `cargo nextest run -p jeryu -- approval::`
//! Invariants:
//!   - No self-approval (author agent cannot be counted in quorum).
//!   - All approvals bind to exact head_sha and policy_sha.
//!   - Veto > approval count: one valid hard-stop rejects regardless of quorum.

pub mod quorum;
pub mod sha_bind;

pub use quorum::{QuorumDecision, QuorumOutcome, evaluate_quorum};
pub use sha_bind::{ShaBindError, verify_sha_binding};
