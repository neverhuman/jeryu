//! Quarantine path: untrusted writes land in the quarantine store, and a
//! separately-gated promotion flow moves a vetted record into the CAS and
//! indexes it in the manifest. Both flows fail closed with signed receipts.

use jeryu_cache_core::{
    CacheEntry, CacheEvent, CacheKey, CacheRequest, Digest, JeryuCacheError, QuarantineRecord,
    Result,
};

use super::JeryuCache;
use crate::service::outcomes::WriteOutcome;

impl JeryuCache {
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
            return Err(JeryuCacheError::PolicyDenied(decision.reasons().join("; ")));
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
            return Err(JeryuCacheError::PolicyDenied(format!(
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
            return Err(JeryuCacheError::PolicyDenied(decision.reasons().join("; ")));
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
}
