//! High-level JeryuCache service API.

use std::path::PathBuf;

use jeryu_cache_core::{
    AccessDecision, CacheAction, CacheEntry, CacheEvent, CacheKey, CacheLayer, CacheManifest,
    CacheReceipt, CacheRequest, ContentAddressedStore, Digest, PolicyEngine, QuarantineRecord,
    QuarantineStore, ReceiptSink, Result, TrustTier, verify_cache_hit,
};

#[derive(Clone, Debug)]
pub struct JeryuCachePaths {
    pub cas_root: PathBuf,
    pub receipt_root: PathBuf,
    pub quarantine_root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct JeryuCache {
    policy: PolicyEngine,
    cas: ContentAddressedStore,
    receipts: ReceiptSink,
    quarantine: QuarantineStore,
    manifest: CacheManifest,
}

#[derive(Clone, Debug)]
pub struct RestoreOutcome {
    pub hit: bool,
    pub object_digest: Option<Digest>,
    pub receipt: CacheReceipt,
}

#[derive(Clone, Debug)]
pub struct WriteOutcome {
    pub object_digest: Digest,
    pub receipt: CacheReceipt,
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

    pub fn restore(&mut self, request: CacheRequest, key: &CacheKey) -> Result<RestoreOutcome> {
        key.verify()?;
        if let Some(denial) = self.key_tier_denial(&request, key) {
            let receipt = self.receipt(
                CacheEvent::Deny,
                Some(key.digest.clone()),
                None,
                &request,
                denial,
            )?;
            return Ok(RestoreOutcome {
                hit: false,
                object_digest: None,
                receipt,
            });
        }

        let decision = self.policy.evaluate(&request);
        if !decision.allowed() {
            let receipt = self.receipt(
                CacheEvent::Deny,
                Some(key.digest.clone()),
                None,
                &request,
                decision,
            )?;
            return Ok(RestoreOutcome {
                hit: false,
                object_digest: None,
                receipt,
            });
        }

        let entry = self.manifest.find(&key.digest, request.layer).cloned();
        match entry {
            Some(entry) => {
                verify_cache_hit(&self.cas, key, &entry.object_digest)?;
                let receipt = self.receipt(
                    CacheEvent::Restore,
                    Some(key.digest.clone()),
                    Some(entry.object_digest.clone()),
                    &request,
                    decision,
                )?;
                Ok(RestoreOutcome {
                    hit: true,
                    object_digest: Some(entry.object_digest),
                    receipt,
                })
            }
            None => {
                let receipt = self.receipt(
                    CacheEvent::SafeMiss,
                    Some(key.digest.clone()),
                    None,
                    &request,
                    decision,
                )?;
                Ok(RestoreOutcome {
                    hit: false,
                    object_digest: None,
                    receipt,
                })
            }
        }
    }

    pub fn write(
        &mut self,
        request: CacheRequest,
        key: CacheKey,
        bytes: &[u8],
    ) -> Result<WriteOutcome> {
        key.verify()?;
        if let Some(denial) = self.key_tier_denial(&request, &key) {
            let receipt = self.receipt(
                CacheEvent::Deny,
                Some(key.digest.clone()),
                None,
                &request,
                denial.clone(),
            )?;
            return Err(jeryu_cache_core::JeryuCacheError::PolicyDenied(format!(
                "{} ({})",
                receipt.receipt_id,
                denial.reasons().join("; ")
            )));
        }

        let decision = self.policy.evaluate(&request);
        if !decision.allowed() {
            let receipt = self.receipt(
                CacheEvent::Deny,
                Some(key.digest.clone()),
                None,
                &request,
                decision.clone(),
            )?;
            return Err(jeryu_cache_core::JeryuCacheError::PolicyDenied(format!(
                "{} ({})",
                receipt.receipt_id,
                decision.reasons().join("; ")
            )));
        }
        let object = self.cas.put_bytes(bytes)?;
        let receipt = self.receipt(
            CacheEvent::Write,
            Some(key.digest.clone()),
            Some(object.digest.clone()),
            &request,
            decision,
        )?;
        self.manifest.add(CacheEntry {
            key,
            object_digest: object.digest.clone(),
            layer: request.layer,
            size_bytes: object.bytes,
            writer_receipt: receipt.clone(),
        });
        Ok(WriteOutcome {
            object_digest: object.digest,
            receipt,
        })
    }

    pub fn quarantine_write(
        &self,
        request: CacheRequest,
        bytes: &[u8],
        reason: &str,
    ) -> Result<QuarantineRecord> {
        let decision = self.policy.evaluate(&request);
        let event = if decision.allowed() {
            CacheEvent::QuarantineWrite
        } else {
            CacheEvent::Deny
        };
        let receipt = self.receipt(event, None, None, &request, decision.clone())?;
        if !decision.allowed() {
            return Err(jeryu_cache_core::JeryuCacheError::PolicyDenied(
                decision.reasons().join("; "),
            ));
        }
        self.quarantine.write(bytes, reason, &receipt)
    }

    pub fn promote_quarantined(
        &mut self,
        request: CacheRequest,
        key: CacheKey,
        record_digest: &Digest,
    ) -> Result<WriteOutcome> {
        key.verify()?;
        if let Some(denial) = self.key_tier_denial(&request, &key) {
            let receipt = self.receipt(
                CacheEvent::Deny,
                Some(key.digest.clone()),
                Some(record_digest.clone()),
                &request,
                denial.clone(),
            )?;
            return Err(jeryu_cache_core::JeryuCacheError::PolicyDenied(format!(
                "{} ({})",
                receipt.receipt_id,
                denial.reasons().join("; ")
            )));
        }

        let decision = self.policy.evaluate(&request);
        let receipt = self.receipt(
            CacheEvent::Promote,
            Some(key.digest.clone()),
            Some(record_digest.clone()),
            &request,
            decision.clone(),
        )?;
        if !decision.allowed() {
            return Err(jeryu_cache_core::JeryuCacheError::PolicyDenied(
                decision.reasons().join("; "),
            ));
        }
        let object = self
            .quarantine
            .promote_to(record_digest, &self.cas, &receipt)?;
        self.manifest.add(CacheEntry {
            key,
            object_digest: object.digest.clone(),
            layer: request.layer,
            size_bytes: object.bytes,
            writer_receipt: receipt.clone(),
        });
        Ok(WriteOutcome {
            object_digest: object.digest,
            receipt,
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
mod tests {
    use tempfile::tempdir;

    use super::*;
    use jeryu_cache_core::{CacheKeyMaterial, CacheScope};

    fn d(label: &str) -> Digest {
        Digest::from_bytes(label.as_bytes())
    }

    fn key_for_tier(trust_tier: TrustTier) -> CacheKey {
        CacheKeyMaterial {
            cache_schema_version: 1,
            tenant_id: "tenant".into(),
            repo_id_or_explicit_shared_scope: "repo".into(),
            trust_tier,
            rustc_version: "rustc".into(),
            cargo_version: "cargo".into(),
            toolchain_channel: "stable".into(),
            host_triple: "host".into(),
            target_triple: "target".into(),
            profile: "dev".into(),
            feature_set: vec![],
            rustflags: vec![],
            linker_identity: "linker".into(),
            sysroot_digest: d("sysroot"),
            cargo_lock_subgraph_digest: d("lock"),
            cargo_toml_digest: d("toml"),
            workspace_metadata_digest: d("workspace"),
            crate_source_digest: d("source"),
            build_rs_digest: d("build"),
            build_rs_declared_inputs_digest: d("inputs"),
            proc_macro_digest: d("proc"),
            native_deps_digest: d("native"),
            env_allowlist_digest: d("env"),
            runner_rootfs_digest: d("rootfs"),
            sandbox_policy_digest: d("sandbox"),
        }
        .derive_key()
        .unwrap()
    }

    fn key() -> CacheKey {
        key_for_tier(TrustTier::T1ProtectedInternal)
    }

    fn request(action: CacheAction) -> CacheRequest {
        CacheRequest {
            action,
            layer: CacheLayer::L3RepoCompiledCas,
            actor_tier: TrustTier::T1ProtectedInternal,
            source_repo_id: "repo".into(),
            target_repo_id: "repo".into(),
            scope: CacheScope::Repo {
                tenant_id: "tenant".into(),
                repo_id: "repo".into(),
            },
            green_protected_policy: true,
            has_explainable_fingerprint: true,
            has_receipt: true,
            is_release_lane: false,
            is_agent_patch: false,
        }
    }

    #[test]
    fn service_write_then_restore_hits() {
        let tmp = tempdir().unwrap();
        let mut service = JeryuCache::open(JeryuCachePaths {
            cas_root: tmp.path().join("cas"),
            receipt_root: tmp.path().join("receipts"),
            quarantine_root: tmp.path().join("quarantine"),
        })
        .unwrap();
        let key = key();
        service
            .write(request(CacheAction::Write), key.clone(), b"artifact")
            .unwrap();
        let restored = service
            .restore(request(CacheAction::Restore), &key)
            .unwrap();
        assert!(restored.hit);
    }

    #[test]
    fn write_rejects_key_request_trust_tier_mismatch() {
        let tmp = tempdir().unwrap();
        let mut service = JeryuCache::open(JeryuCachePaths {
            cas_root: tmp.path().join("cas"),
            receipt_root: tmp.path().join("receipts"),
            quarantine_root: tmp.path().join("quarantine"),
        })
        .unwrap();
        let mut request = request(CacheAction::Write);
        request.actor_tier = TrustTier::T2InternalBranch;
        let err = service.write(request, key(), b"artifact").unwrap_err();
        assert!(err.to_string().contains("trust tier"));
    }

    #[test]
    fn restore_denies_key_request_trust_tier_mismatch() {
        let tmp = tempdir().unwrap();
        let mut service = JeryuCache::open(JeryuCachePaths {
            cas_root: tmp.path().join("cas"),
            receipt_root: tmp.path().join("receipts"),
            quarantine_root: tmp.path().join("quarantine"),
        })
        .unwrap();
        let key = key();
        service
            .write(request(CacheAction::Write), key.clone(), b"artifact")
            .unwrap();
        let mut request = request(CacheAction::Restore);
        request.actor_tier = TrustTier::T2InternalBranch;
        let restored = service.restore(request, &key).unwrap();
        assert!(!restored.hit);
        assert!(!restored.receipt.decision.allowed());
    }

    #[test]
    fn promotion_indexes_manifest_for_future_restore() {
        let tmp = tempdir().unwrap();
        let mut service = JeryuCache::open(JeryuCachePaths {
            cas_root: tmp.path().join("cas"),
            receipt_root: tmp.path().join("receipts"),
            quarantine_root: tmp.path().join("quarantine"),
        })
        .unwrap();
        let key = key();
        let mut quarantine = request(CacheAction::QuarantineWrite);
        quarantine.is_agent_patch = true;
        let record = service
            .quarantine_write(quarantine, b"artifact", "agent patch")
            .unwrap();
        service
            .promote_quarantined(
                request(CacheAction::Promote),
                key.clone(),
                &record.object.digest,
            )
            .unwrap();
        let restored = service
            .restore(request(CacheAction::Restore), &key)
            .unwrap();
        assert!(restored.hit);
    }
}
