//! `jeryu-autonomy` — the Evidence-Gate autonomous-delivery control plane.
//!
//! Every agent-authored PR is reduced to a signed, SHA-bound [`EvidencePack`],
//! reviewed by quorum reviewer agents that each emit a signed
//! [`AgentApprovalReceipt`], fused by the *pure* [`judge`](judge::judge) into a
//! signed [`VibeGateVerdict`] (AllowMerge / RequireHuman / Reject). A named
//! hard-stop [`ConditionRegistry`](conditions::ConditionRegistry) can veto any
//! verdict (veto > approval). Every decision is recorded append-only in the
//! signed launch ledger; a [`KillBell`](kill_bell::KillBell) can globally
//! downgrade verdicts to RequireHuman; drift detection re-judges outdated
//! verdicts; escalation fans out RequireHuman / KillBellEngaged events.
//!
//! ## Layering
//!
//! - **Pure decision core** (no IO): [`conditions`], [`quorum`], [`sha_bind`],
//!   [`judge`], [`rejudge`], [`risk`-free `freeze`](freeze) — these are the
//!   substance that, in the fused product, moves to `jeryu-proof`.
//! - **Typed model**: [`types`] (the 8 canonical objects), [`policy_yaml`].
//! - **Stateful / side-effecting** behind object-safe seams ([`seam`]):
//!   [`ledger`], [`verdict_store`], [`kill_bell`], [`escalation`],
//!   [`auto_rejudge`], [`replay`]. In-memory implementations ([`clock`],
//!   `MemoryLedger`, `MemoryVerdictStore`) preserve every load-bearing
//!   invariant; the fused product swaps in forge/DB-backed implementations.
//!
//! Only `jeryu` / `jeryu-*` names appear in this crate; no external-host or
//! legacy-product brand literals are present.

#![forbid(unsafe_code)]

pub mod clock;
pub mod conditions;
pub mod escalation;
pub mod evidence;
pub mod freeze;
pub mod full_auto;
pub mod judge;
pub mod kill_bell;
pub mod ledger;
pub mod policy_yaml;
pub mod policy_yaml_types;
pub mod quorum;
pub mod rejudge;
pub mod seam;
pub mod sha_bind;
pub mod signing;
pub mod types;
pub mod verdict_store;

#[cfg(test)]
mod test_support;

// --- Curated re-exports (the public Evidence-Gate surface) -----------------

pub use auto_rejudge::{AutoRejudgeService, RejudgeOutcome};
pub use clock::{FixedClock, SystemClock};
pub use conditions::{ConditionRegistry, HardStop, NamedCondition};
pub use escalation::{
    DispatchResult, EscalationConfig, EscalationError, EscalationEvent, EscalationKind,
    WebhookConfig, build_payload, dispatch_all,
};
pub use evidence::{
    EvidenceInputs, build_evidence_pack, make_gate_receipt, verify_evidence_digest,
};
pub use freeze::{FreezeWindow, FreezeWindows};
pub use full_auto::{
    FULL_AUTO_MAX_AUTO_TIER, FULL_AUTO_SCHEMA, FullAutoDescriptor, FullAutoError, FullAutoProfile,
};
pub use judge::{JudgeInputs, JudgeOutcome, judge};
pub use kill_bell::{BreakGlassReceipt, KillBell, KillBellState};
pub use ledger::{
    MemoryLedger, canonical_body_for_signing, kind_from_str, kind_to_str, sign_entry,
    verdict_issued_entry,
};
pub use policy_yaml::PolicyBundle;
pub use quorum::{QuorumDecision, QuorumOutcome, evaluate_quorum};
pub use rejudge::{LiveState, RejudgeReason, check, must_rejudge};
pub use replay::{ReplayReport, ReplaySummary, TimelineEvent, replay_subject};
pub use seam::{
    Clock, EscalationSink, EvidenceSource, LedgerFilter, SeamError, SeamResult, VerdictLedger,
    VerdictStore,
};
pub use sha_bind::{ShaBindError, verify_sha_binding};
pub use signing::{EdSigningKey, EdVerifier, Signature, SigningKey, sha256_digest};
pub use types::{
    AgentApprovalReceipt, CapabilityLease, ChangedFile, CiCheck, CiConclusion, EvidencePack,
    Finding, GateDecision, GateReceipt, IntentCard, LaunchLedgerEntry, LeaseDenied, LeaseScope,
    LedgerKind, MergePassport, ReleasePassport, ReviewDecision, ReviewerRole, RiskTier,
    RollbackSection, RollbackStrategy, ScanOutcome, SchemaTag, SecuritySection, Severity,
    SupplyChainSection, TestsSection, TokenCounts, VerdictReceiptRef, VibeGateVerdict,
};
pub use verdict_store::MemoryVerdictStore;

pub mod auto_rejudge;
pub mod replay;
