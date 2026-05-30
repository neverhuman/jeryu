//! Thin trait seams standing in for the future `jeryu-*` core / forge / store.
//!
//! This crate ports the Evidence-Gate decision substance faithfully but must
//! stay self-contained. Every external system (the forge PR surface, the
//! proof/CI receipt source, the durable ledger, the clock, the escalation
//! transport) is reached through ONE of the object-safe traits below. The
//! crate ships in-memory implementations ([`crate::store`], [`crate::clock`])
//! that preserve every load-bearing invariant; the fused product swaps in
//! forge/DB-backed implementations behind the same traits.

use crate::types::{AgentApprovalReceipt, EvidencePack, LaunchLedgerEntry, LedgerKind};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Typed seam error. Carries enough context for a degraded badge, never a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeamError {
    pub source: &'static str,
    pub reason: String,
}

impl SeamError {
    pub fn new(source: &'static str, reason: impl Into<String>) -> Self {
        Self {
            source,
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for SeamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.source, self.reason)
    }
}

impl std::error::Error for SeamError {}

pub type SeamResult<T> = Result<T, SeamError>;

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// Time source. The control plane reads `now()` instead of `Utc::now()` so that
/// TTL auto-arm, verdict expiry, and freeze windows are deterministically
/// testable. Object-safe.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

// ---------------------------------------------------------------------------
// Evidence source (forge PR + proof/CI receipts)
// ---------------------------------------------------------------------------

/// The producing surface that turns a forge PR + proof/CI run into a signed
/// [`EvidencePack`] and the reviewer receipts bound to it. Replaces the source
/// product's host adapter; backed in the fused product by the forge PR/diff
/// surface and the jeryu-ci run-status query.
#[async_trait]
pub trait EvidenceSource: Send + Sync {
    /// Build (or fetch) the signed evidence pack for one PR head.
    async fn build_pack(&self, repo: &str, pr_id: &str) -> SeamResult<EvidencePack>;

    /// Run the configured reviewer roles against the pack and return their
    /// signed receipts. An orchestrator-level failure surfaces as `Err`; the
    /// caller (auto-rejudge) decides whether to degrade to "no receipts".
    async fn run_reviews(
        &self,
        pack: &EvidencePack,
        required_roles: &[crate::types::ReviewerRole],
    ) -> SeamResult<Vec<AgentApprovalReceipt>>;
}

// ---------------------------------------------------------------------------
// Verdict ledger (append-only, signed audit trail)
// ---------------------------------------------------------------------------

/// Filter for ledger reads. Mirrors the SQL `LedgerFilter`.
#[derive(Debug, Clone, Default)]
pub struct LedgerFilter {
    pub kind: Option<LedgerKind>,
    pub subject_id: Option<String>,
    pub repo: Option<String>,
    pub limit: Option<i64>,
}

/// The append-only, ed25519-only signed launch ledger. Object-safe. The
/// in-memory implementation in [`crate::ledger`] enforces the same invariants
/// the SQL `BEFORE UPDATE/DELETE` triggers do: no mutation after append,
/// idempotency on id, and refusal of unsigned/HMAC signatures.
#[async_trait]
pub trait VerdictLedger: Send + Sync {
    /// Append one entry. Refuses unsigned/HMAC signatures. Idempotent on `entry.id`.
    async fn append(&self, entry: &LaunchLedgerEntry) -> SeamResult<()>;

    /// Return entries matching the filter, oldest first (recorded_at ASC).
    async fn list(&self, filter: &LedgerFilter) -> SeamResult<Vec<LaunchLedgerEntry>>;
}

// ---------------------------------------------------------------------------
// Verdict store (live-verdict projection)
// ---------------------------------------------------------------------------

/// The live-verdict projection the daemon polls. Does NOT enforce signing
/// (that is the ledger's job); the in-memory implementation in
/// [`crate::verdict_store`] enforces idempotent save + supersede + the
/// `list_active` filters.
#[async_trait]
pub trait VerdictStore: Send + Sync {
    /// Persist a verdict. Idempotent on `verdict.id`. Marks prior
    /// non-superseded rows for the same (repo, pull_request) pair as superseded.
    async fn save(&self, verdict: &crate::types::VibeGateVerdict) -> SeamResult<()>;

    /// Most-recent non-superseded verdict for a (repo, pull_request) pair.
    async fn load_latest(
        &self,
        repo: &str,
        pull_request: Option<&str>,
    ) -> SeamResult<Option<crate::types::VibeGateVerdict>>;

    /// All currently-active verdicts: not superseded, not expired, not rejected.
    /// Ordered by created_at ascending.
    async fn list_active(
        &self,
        now: DateTime<Utc>,
    ) -> SeamResult<Vec<crate::types::VibeGateVerdict>>;

    /// Mark one verdict row as superseded. No-op if already superseded/unknown.
    async fn supersede(&self, verdict_id: &str, now: DateTime<Utc>) -> SeamResult<()>;
}

// ---------------------------------------------------------------------------
// Escalation sink (webhook transport)
// ---------------------------------------------------------------------------

/// The "Needs You" transport: where `RequireHuman` verdicts and
/// `KillBellEngaged` events go. The fused product backs this with the HTTP
/// dispatcher; the in-memory implementation records calls for inspection. A
/// failure on one sink NEVER aborts the others (the fan-out lives in
/// [`crate::escalation::dispatch_all`]).
#[async_trait]
pub trait EscalationSink: Send + Sync {
    /// Deliver one already-shaped payload to one webhook. Returns the HTTP-like
    /// status code on success.
    async fn deliver(
        &self,
        webhook: &crate::escalation::WebhookConfig,
        payload: serde_json::Value,
    ) -> Result<u16, crate::escalation::EscalationError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;

    #[test]
    fn seam_traits_are_object_safe() {
        // Clock is object-safe.
        let c = FixedClock::new(Utc::now());
        let _dyn_clock: &dyn Clock = &c;
        // SeamError displays.
        let e = SeamError::new("forge", "pr not found");
        assert_eq!(e.to_string(), "forge: pr not found");
    }
}
