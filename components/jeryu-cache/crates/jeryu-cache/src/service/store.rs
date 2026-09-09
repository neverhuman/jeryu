//! Cache store path.

use super::{JeryuCache, StoreOutcome, StoreStatus, manifest_digest};
use crate::cache::{CacheEntry, CacheEntryState, CacheKind, CacheScope};
use crate::cas::CasBackend;
use crate::error::Result;
use crate::fingerprint::FingerprintInput;
use crate::ids::Actor;
use crate::policy::{CacheAccess, CacheContext, CacheDisposition};
use crate::receipt::CacheAction;

impl<B: CasBackend> JeryuCache<B> {
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
        let decision = self.policy().evaluate(
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
}
