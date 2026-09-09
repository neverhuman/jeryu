//! High-level JeryuCache service.

mod restore;
mod store;

use crate::cache::{CacheEntry, CacheKey, CacheKind, CacheScope, FsIndex};
use crate::cas::{CasBackend, FsCas};
use crate::digest::{Digest, digest_bytes};
use crate::error::Result;
use crate::fingerprint::FingerprintInput;
use crate::ids::Actor;
use crate::policy::{CacheContext, CachePolicy, PolicyDecision};
use crate::receipt::{CacheAction, CacheReceipt, ReceiptLog};
use std::path::Path;

/// Store outcome status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreStatus {
    /// Stored directly as promoted/trusted.
    Trusted,
    /// Stored into quarantine.
    Quarantined,
    /// Denied by policy.
    Denied,
}

/// Restore outcome status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreStatus {
    /// Restored trusted bytes.
    Hit,
    /// No index/object exists.
    Miss,
    /// Policy or outage produced a safe miss.
    SafeMiss,
    /// Entry exists but is quarantined.
    Quarantined,
    /// Poison/false-hit was detected.
    Poisoned,
}

/// Result of a store attempt.
#[derive(Debug, Clone)]
pub struct StoreOutcome {
    /// Store status.
    pub status: StoreStatus,
    /// Stored entry when available.
    pub entry: Option<CacheEntry>,
    /// Receipt for the decision.
    pub receipt: CacheReceipt,
}

/// Result of a restore attempt.
#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    /// Restore status.
    pub status: RestoreStatus,
    /// Restored bytes for hits.
    pub bytes: Option<Vec<u8>>,
    /// Receipt for the decision.
    pub receipt: CacheReceipt,
}

/// Result of promotion.
#[derive(Debug, Clone)]
pub struct PromoteOutcome {
    /// Promoted entry.
    pub entry: Option<CacheEntry>,
    /// Receipt for the decision.
    pub receipt: CacheReceipt,
}

/// JeryuCache service.
#[derive(Debug, Clone)]
pub struct JeryuCache<B: CasBackend> {
    cas: B,
    index: FsIndex,
    receipts: ReceiptLog,
    policy: CachePolicy,
}

impl JeryuCache<FsCas> {
    /// Opens a filesystem-backed vault under `root`.
    pub fn open(root: impl AsRef<Path>, policy: CachePolicy) -> Result<Self> {
        let root = root.as_ref();
        let cas = FsCas::open(root.join("cas"))?;
        Self::with_backend(root, cas, policy)
    }
}

impl<B: CasBackend> JeryuCache<B> {
    /// Opens a vault with a custom CAS backend.
    pub fn with_backend(root: impl AsRef<Path>, cas: B, policy: CachePolicy) -> Result<Self> {
        let root = root.as_ref();
        Ok(Self {
            cas,
            index: FsIndex::open(root.join("index"))?,
            receipts: ReceiptLog::open(root.join("receipts"))?,
            policy,
        })
    }

    /// Access to the underlying CAS backend for tests/tools.
    pub fn cas(&self) -> &B {
        &self.cas
    }

    /// Returns an immutable reference to policy.
    pub fn policy(&self) -> &CachePolicy {
        &self.policy
    }

    /// Builds a cache key from kind, scope, and fingerprint input.
    pub fn key_for(
        &self,
        kind: CacheKind,
        scope: CacheScope,
        fingerprint: &FingerprintInput,
    ) -> CacheKey {
        CacheKey::new(kind, scope, fingerprint.digest())
    }

    fn receipt(
        &self,
        actor: Actor,
        action: CacheAction,
        key: &CacheKey,
        object_digest: Option<Digest>,
        decision: &PolicyDecision,
        context: &CacheContext,
    ) -> Result<CacheReceipt> {
        let receipt = CacheReceipt::new(
            actor,
            action,
            key,
            object_digest,
            decision,
            context.trust_tier,
        );
        self.receipts.append(&receipt)?;
        Ok(receipt)
    }
}

/// Manifest digest binds logical key, object digest, and payload size.
pub fn manifest_digest(key: &CacheKey, object_digest: &Digest, size: usize) -> Digest {
    digest_bytes(
        format!(
            "key={}\nobject={}\nsize={}\n",
            key.digest(),
            object_digest,
            size
        )
        .as_bytes(),
    )
}
