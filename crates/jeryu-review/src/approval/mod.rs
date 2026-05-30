//! Approval subsystem: SHA binding (Law 4) + quorum voting.
//!
//! Invariants documented across the two modules:
//!   - No self-approval; distinct agent identities required.
//!   - Exact-SHA binding: a receipt is valid for exactly one
//!     `(evidence_pack_id, head_sha, policy_sha)` tuple.
//!   - Veto > approval count: a single `Block` vetoes the quorum.

pub mod quorum;
pub mod sha_bind;

pub use quorum::{QuorumDecision, QuorumOutcome, evaluate_quorum};
pub use sha_bind::{ShaBindError, verify_sha_binding};
