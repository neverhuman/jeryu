//! Cache restore, promotion, and false-hit detection.

use super::{JeryuCache, PromoteOutcome, RestoreOutcome, RestoreStatus};
use crate::cache::{CacheEntryState, CacheKey};
use crate::digest::{Digest, digest_bytes};
use crate::error::{Result, VaultError};
use crate::false_hit::{FalseHitDetector, FalseHitReport};
use crate::ids::Actor;
use crate::policy::{CacheAccess, CacheContext, CacheDisposition, PolicyDecision};
use crate::receipt::CacheAction;

impl<B: crate::cas::CasBackend> JeryuCache<B> {
    /// Restores a cache payload after policy evaluation.
    pub fn restore_cache(
        &self,
        context: &CacheContext,
        actor: Actor,
        key: &CacheKey,
    ) -> Result<RestoreOutcome> {
        let decision = self
            .policy()
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
        let decision = self.policy().evaluate(
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
}
