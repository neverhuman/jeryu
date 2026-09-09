//! Write path: trust-tier gate, policy evaluation, CAS put, and manifest
//! indexing. Denials fail closed with a signed receipt referenced in the error.

use jeryu_cache_core::{CacheEntry, CacheEvent, CacheKey, CacheRequest, JeryuCacheError, Result};

use super::JeryuCache;
use crate::service::outcomes::WriteOutcome;

impl JeryuCache {
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
            return Err(JeryuCacheError::PolicyDenied(format!(
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
            return Err(JeryuCacheError::PolicyDenied(format!(
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
}
