//! Owner: Evidence Gate / autonomy control plane
//! Proof: `cargo nextest run -p jeryu -- autonomy::`
//! Invariants:
//!   - All 8 canonical objects round-trip through `serde_json` losslessly.
//!   - Receipts/verdicts/passports are immutable; mutation = new id.
//!   - Hard-stop logic lives in `conditions.rs` only — no string-eval, ever.
//!
//! Implements the typed object model and policy fusion for the
//! "Evidence Gate" / VibeGate Delivery Spine standard.
//! See `tips/fullauto/tip1.txt` for the controlling design and
//! `.autonomy/schemas/*.schema.json` for the on-the-wire schemas.

pub mod auto_rejudge; // Wave 8.C — auto-rejudge service composer. Re-exports
// handled by Wave 8.E integration.
pub mod conditions;
pub mod daemon;
pub mod escalation;
pub mod escalation_loader;
pub mod evidence;
pub mod evidence_pack_builder;
pub mod freeze;
pub mod http_server;
pub mod kill_bell;
pub mod ledger;
pub mod mcp_tools;
pub mod metrics;
pub mod policy_yaml;
pub mod profile;
pub mod replay;
pub mod risk;
pub mod shadow;
pub mod signing;
pub mod types;
pub mod verdict_store;

pub use conditions::{ConditionRegistry, HardStop, NamedCondition};
pub use evidence::{
    EvidenceInputs, build_evidence_pack, make_legacy_receipt, verify_evidence_digest,
};
pub use ledger::{LedgerFilter, SqlLedger, sign_entry, verdict_issued_entry};
pub use policy_yaml::{ApprovalsPolicy, PolicyBundle, ReleasePolicy, RiskPolicy};
pub use risk::{ClassificationInputs, RiskClassifier, compile_glob};
pub use signing::{Signature, SigningKey, sha256_digest};
pub use types::{
    AgentApprovalReceipt, CapabilityLease, EvidencePack, IntentCard, LaunchLedgerEntry,
    MergePassport, ReleasePassport, VibeGateVerdict,
};
