//! The [`JeryuCache`] service: trust-tier-gated restore, write, quarantine, and
//! promotion flows that fail closed and emit a signed receipt for every event.
//!
//! The flow methods are split across responsibility-scoped submodules that each
//! add an `impl JeryuCache` block:
//! - [`restore`] — read path (trust-tier gate, policy, verified hit / safe miss).
//! - [`write`] — write path (gate, policy, CAS put, manifest index).
//! - [`quarantine`] — quarantine write + promotion flows.
//!
//! This module owns the struct, constructor, manifest accessor, and the private
//! plumbing (trust-tier denial + signed-receipt emission) shared by all flows.

mod quarantine;
mod restore;
mod write;

use jeryu_cache_core::{
    AccessDecision, CacheAction, CacheEvent, CacheKey, CacheLayer, CacheManifest, CacheReceipt,
    CacheRequest, ContentAddressedStore, Digest, PolicyEngine, QuarantineStore, ReceiptSink,
    Result, TrustTier,
};

use super::outcomes::JeryuCachePaths;

#[derive(Clone, Debug)]
pub struct JeryuCache {
    policy: PolicyEngine,
    cas: ContentAddressedStore,
    receipts: ReceiptSink,
    quarantine: QuarantineStore,
    manifest: CacheManifest,
}

impl JeryuCache {
    pub fn open(paths: JeryuCachePaths) -> Result<Self> {
        Ok(Self {
            policy: PolicyEngine,
            cas: ContentAddressedStore::open(paths.cas_root)?,
            receipts: ReceiptSink::open(paths.receipt_root)?,
            quarantine: QuarantineStore::open(paths.quarantine_root)?,
            manifest: CacheManifest::default(),
        })
    }

    pub fn manifest(&self) -> &CacheManifest {
        &self.manifest
    }

    fn key_tier_denial(&self, request: &CacheRequest, key: &CacheKey) -> Option<AccessDecision> {
        if key.material.trust_tier == request.actor_tier {
            return None;
        }
        Some(AccessDecision::Deny {
            reasons: vec![format!(
                "CV-LAW-001: cache key trust tier {} does not match request actor tier {}",
                key.material.trust_tier, request.actor_tier
            )],
        })
    }

    fn receipt(
        &self,
        event: CacheEvent,
        key_digest: Option<Digest>,
        object_digest: Option<Digest>,
        request: &CacheRequest,
        decision: AccessDecision,
    ) -> Result<CacheReceipt> {
        let action: CacheAction = request.action;
        let layer: CacheLayer = request.layer;
        let actor_tier: TrustTier = request.actor_tier;
        let receipt = CacheReceipt::new(
            event,
            key_digest,
            object_digest,
            action,
            layer,
            actor_tier,
            decision,
            None,
        )?;
        self.receipts.write(&receipt)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
