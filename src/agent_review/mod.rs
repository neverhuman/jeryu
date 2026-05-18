//! Owner: Evidence Gate / reviewer agents
//! Proof: `cargo nextest run -p jeryu -- agent_review::`
//! Invariants:
//!   - Diff bytes always wrapped in `<diff>...</diff>` and described as untrusted input.
//!   - Reviewer output is parsed against a strict schema; malformed → `abstain` receipt.
//!   - Judge agent never reads code (lives elsewhere; pure policy fusion).
//!   - Every receipt records `prompt_sha`, `model`, `provider`, `raw_response_sha`
//!     for replay/audit.

pub mod judge;
pub mod lockfile;
pub mod nightwatch;
pub mod orchestrator;
pub mod parse;
pub mod prompt_builder;
pub mod rejudge;
pub mod runner;
pub mod runtime;
pub mod security;
pub mod test_integrity;

pub use judge::{JudgeInputs, JudgeOutcome, judge};
pub use lockfile::{LockfileReviewInputs, run_lockfile_review};
pub use nightwatch::{NightwatchReviewInputs, run_nightwatch_review};
pub use parse::{ParsedReceiptFields, extract_receipt_json};
pub use prompt_builder::{ReviewerPromptInputs, build_reviewer_messages, prompt_sha};
pub use rejudge::{LiveState, RejudgeReason, check as rejudge_check, must_rejudge};
pub use runner::{ReviewInputs, ReviewerRoleId, run_review};
pub use runtime::{RuntimeReviewInputs, run_runtime_review};
pub use security::{ReviewerCallError, run_security_review};
pub use test_integrity::{TestIntegrityReviewInputs, run_test_integrity_review};
