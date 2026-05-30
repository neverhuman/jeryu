//! High-level CrateVault service.

use crate::cache::{CacheEntry, CacheEntryState, CacheKey, CacheKind, CacheScope, FsIndex};
use crate::cas::{CasBackend, FsCas};
use crate::digest::{digest_bytes, Digest};
use crate::error::{Result, VaultError};
use crate::false_hit::{FalseHitDetector, FalseHitReport};
use crate::fingerprint::FingerprintInput;
use crate::ids::Actor;
use crate::policy::{CacheAccess, CacheContext, CacheDisposition, CachePolicy, PolicyDecision};
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

/// CrateVault service.
#[derive(Debug, Clone)]
pub struct CrateVault<B: CasBackend> {
    cas: B,
    index: FsIndex,
    receipts: ReceiptLog,
    policy: CachePolicy,
}

impl CrateVault<FsCas> {
    /// Opens a filesystem-backed vault under `root`.
    pub fn open(root: impl AsRef<Path>, policy: CachePolicy) -> Result<Self> {
        let root = root.as_ref();
        let cas = FsCas::open(root.join("cas"))?;
        Self::with_backend(root, cas, policy)
    }
}

impl<B: CasBackend> CrateVault<B> {
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

    /// Stores a cache payload after policy evaluation.
    #[allow(clippy::too_many_arguments)]
    pub fn put_cache(
        &self,
        context: &CacheContext,
        actor: Actor,
        kind: CacheKind,
        scope: CacheScope,
        fingerprint: &FingerprintInput,
        bytes: &[u8],
        ci_green: bool,
    ) -> Result<StoreOutcome> {
        let key = self.key_for(kind, scope, fingerprint);
        let decision = self.policy.evaluate(
            context,
            &key.scope,
            key.kind,
            CacheAccess::Write { ci_green },
        );
        if !decision.allowed && decision.disposition == CacheDisposition::Deny {
            let receipt =
                self.receipt(actor, CacheAction::Denied, &key, None, &decision, context)?;
            return Ok(StoreOutcome {
                status: StoreStatus::Denied,
                entry: None,
                receipt,
            });
        }
        if !decision.allowed {
            let receipt =
                self.receipt(actor, CacheAction::SafeMiss, &key, None, &decision, context)?;
            return Ok(StoreOutcome {
                status: StoreStatus::Denied,
                entry: None,
                receipt,
            });
        }

        let object_digest = self.cas.put(bytes)?;
        let manifest_digest = manifest_digest(&key, &object_digest, bytes.len());
        let state = match decision.disposition {
            CacheDisposition::Trusted => CacheEntryState::Promoted,
            CacheDisposition::Quarantine => CacheEntryState::Quarantined,
            CacheDisposition::SafeMiss
            | CacheDisposition::Deny
            | CacheDisposition::IgnoredMutableCache => CacheEntryState::Quarantined,
        };
        let entry = CacheEntry::new(
            key.clone(),
            object_digest.clone(),
            manifest_digest,
            state,
            context.trust_tier,
        );
        self.index.put(&entry)?;
        let action = if state == CacheEntryState::Promoted {
            CacheAction::Write
        } else {
            CacheAction::Quarantine
        };
        let receipt = self.receipt(actor, action, &key, Some(object_digest), &decision, context)?;
        Ok(StoreOutcome {
            status: if state == CacheEntryState::Promoted {
                StoreStatus::Trusted
            } else {
                StoreStatus::Quarantined
            },
            entry: Some(entry),
            receipt,
        })
    }

    /// Restores a cache payload after policy evaluation.
    pub fn restore_cache(
        &self,
        context: &CacheContext,
        actor: Actor,
        key: &CacheKey,
    ) -> Result<RestoreOutcome> {
        let decision = self
            .policy
            .evaluate(context, &key.scope, key.kind, CacheAccess::Read);
        if !decision.allowed {
            let receipt =
                self.receipt(actor, CacheAction::SafeMiss, key, None, &decision, context)?;
            return Ok(RestoreOutcome {
                status: RestoreStatus::SafeMiss,
                bytes: None,
                receipt,
            });
        }

        let Some(entry) = self.index.get(key)? else {
            let miss = PolicyDecision {
                allowed: true,
                disposition: CacheDisposition::SafeMiss,
                law: "cache-miss".to_string(),
                reason: "no cache entry exists for key".to_string(),
            };
            let receipt = self.receipt(actor, CacheAction::SafeMiss, key, None, &miss, context)?;
            return Ok(RestoreOutcome {
                status: RestoreStatus::Miss,
                bytes: None,
                receipt,
            });
        };

        if entry.state == CacheEntryState::Quarantined {
            let quarantine = PolicyDecision {
                allowed: false,
                disposition: CacheDisposition::SafeMiss,
                law: "quarantine-not-restorable".to_string(),
                reason: "quarantined entries require promotion before restore".to_string(),
            };
            let receipt = self.receipt(
                actor,
                CacheAction::SafeMiss,
                key,
                Some(entry.object_digest),
                &quarantine,
                context,
            )?;
            return Ok(RestoreOutcome {
                status: RestoreStatus::Quarantined,
                bytes: None,
                receipt,
            });
        }

        let bytes = match self.cas.get(&entry.object_digest) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                let miss = PolicyDecision {
                    allowed: true,
                    disposition: CacheDisposition::SafeMiss,
                    law: "cas-object-miss".to_string(),
                    reason: "cache index exists but CAS object is missing".to_string(),
                };
                let receipt = self.receipt(
                    actor,
                    CacheAction::SafeMiss,
                    key,
                    Some(entry.object_digest),
                    &miss,
                    context,
                )?;
                return Ok(RestoreOutcome {
                    status: RestoreStatus::Miss,
                    bytes: None,
                    receipt,
                });
            }
            Err(VaultError::StoreUnavailable) => {
                let outage = PolicyDecision {
                    allowed: false,
                    disposition: CacheDisposition::SafeMiss,
                    law: "cache-service-outage-safe-miss".to_string(),
                    reason: "CAS backend unavailable; read degraded to safe miss".to_string(),
                };
                let receipt = self.receipt(
                    actor,
                    CacheAction::SafeMiss,
                    key,
                    Some(entry.object_digest),
                    &outage,
                    context,
                )?;
                return Ok(RestoreOutcome {
                    status: RestoreStatus::SafeMiss,
                    bytes: None,
                    receipt,
                });
            }
            Err(err) => return Err(err),
        };

        if digest_bytes(&bytes) != entry.object_digest {
            let poison = PolicyDecision {
                allowed: false,
                disposition: CacheDisposition::Deny,
                law: "no-false-hits-tolerated".to_string(),
                reason: "restored bytes do not match indexed object digest".to_string(),
            };
            let _ = self.index.set_state(key, CacheEntryState::Quarantined)?;
            let receipt = self.receipt(
                actor,
                CacheAction::PoisonDetected,
                key,
                Some(entry.object_digest),
                &poison,
                context,
            )?;
            return Ok(RestoreOutcome {
                status: RestoreStatus::Poisoned,
                bytes: None,
                receipt,
            });
        }

        let receipt = self.receipt(
            actor,
            CacheAction::Read,
            key,
            Some(entry.object_digest),
            &decision,
            context,
        )?;
        Ok(RestoreOutcome {
            status: RestoreStatus::Hit,
            bytes: Some(bytes),
            receipt,
        })
    }

    /// Promotes a quarantined cache entry after protected green policy.
    pub fn promote_cache(
        &self,
        context: &CacheContext,
        actor: Actor,
        key: &CacheKey,
        ci_green: bool,
    ) -> Result<PromoteOutcome> {
        let decision = self.policy.evaluate(
            context,
            &key.scope,
            key.kind,
            CacheAccess::Promote { ci_green },
        );
        if !decision.allowed {
            let receipt =
                self.receipt(actor, CacheAction::Denied, key, None, &decision, context)?;
            return Ok(PromoteOutcome {
                entry: None,
                receipt,
            });
        }

        let entry = self.index.set_state(key, CacheEntryState::Promoted)?;
        let receipt = self.receipt(actor, CacheAction::Promote, key, None, &decision, context)?;
        Ok(PromoteOutcome { entry, receipt })
    }

    /// Verifies an entry against an expected manifest and quarantines on mismatch.
    pub fn detect_false_hit(
        &self,
        context: &CacheContext,
        actor: Actor,
        key: &CacheKey,
        expected_manifest_digest: &Digest,
    ) -> Result<FalseHitReport> {
        let Some(entry) = self.index.get(key)? else {
            return Ok(FalseHitReport {
                clean: true,
                reason: "no entry exists".to_string(),
            });
        };
        let bytes = match self.cas.get(&entry.object_digest) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => Vec::new(),
            Err(VaultError::StoreUnavailable) => Vec::new(),
            Err(err) => return Err(err),
        };
        let report = FalseHitDetector::verify(&entry, &bytes, expected_manifest_digest);
        if !report.clean {
            let _ = self.index.set_state(key, CacheEntryState::Quarantined)?;
            let decision = PolicyDecision {
                allowed: false,
                disposition: CacheDisposition::Deny,
                law: "no-false-hits-tolerated".to_string(),
                reason: report.reason.clone(),
            };
            let _receipt = self.receipt(
                actor,
                CacheAction::PoisonDetected,
                key,
                Some(entry.object_digest),
                &decision,
                context,
            )?;
        }
        Ok(report)
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
