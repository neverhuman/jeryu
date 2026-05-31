//! Read path: trust-tier gate, policy evaluation, then either a CAS-verified
//! hit or a safe miss. Every outcome emits a signed receipt.

use jeryu_cache_core::{CacheEvent, CacheKey, CacheRequest, Result, verify_cache_hit};

use super::JeryuCache;
use crate::service::outcomes::RestoreOutcome;

impl JeryuCache {
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
}
