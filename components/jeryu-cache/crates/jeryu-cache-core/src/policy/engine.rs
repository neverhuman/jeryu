use super::types::{AccessDecision, CacheAction, CacheLayer, CacheRequest};
use crate::tier::TrustTier;

#[derive(Clone, Debug, Default)]
pub struct PolicyEngine;

impl PolicyEngine {
    pub fn evaluate(&self, request: &CacheRequest) -> AccessDecision {
        let mut deny = Vec::<String>::new();
        let mut allow = Vec::<String>::new();

        if matches!(request.action, CacheAction::Restore | CacheAction::Read)
            && !request.has_explainable_fingerprint
        {
            deny.push("CV-LAW-008: restore/read requires explainable fingerprint".into());
        }

        if request.is_release_lane
            && request.layer.is_compiled()
            && request.layer.is_mutable()
            && matches!(request.action, CacheAction::Read | CacheAction::Restore)
        {
            deny.push("CV-LAW-004: release lane cannot consume mutable compiled cache".into());
        }

        if matches!(request.actor_tier, TrustTier::T0ReleaseHermetic)
            && !matches!(request.layer, CacheLayer::L6ReleaseHermeticVendorSnapshot)
            && matches!(request.action, CacheAction::Read | CacheAction::Restore)
        {
            deny.push("T0 release-hermetic lanes may restore only L6 hermetic snapshots".into());
        }

        if request.actor_tier.is_untrusted()
            && request.layer.is_trusted_compiled()
            && matches!(request.action, CacheAction::Read | CacheAction::Restore)
        {
            deny.push("CV-LAW-001/CV-LAW-003: untrusted jobs may read source caches only, not trusted compiled cache".into());
        }

        if request.actor_tier.is_untrusted()
            && request.layer.is_trusted_compiled()
            && matches!(request.action, CacheAction::Write | CacheAction::Promote)
        {
            deny.push(
                "CV-LAW-002/CV-LAW-003: untrusted jobs cannot write trusted compiled cache".into(),
            );
        }

        if matches!(request.action, CacheAction::Promote) && !request.has_receipt {
            deny.push("CV-LAW-009: cache promotion requires receipt".into());
        }

        if matches!(request.action, CacheAction::Write | CacheAction::Promote)
            && request.layer.is_trusted_compiled()
            && !request.actor_tier.can_write_trusted_compiled_cache()
        {
            deny.push("only T1 protected-internal can write trusted compiled cache".into());
        }

        if matches!(request.action, CacheAction::Write | CacheAction::Promote)
            && request.layer.is_trusted_compiled()
            && request.actor_tier.can_write_trusted_compiled_cache()
            && !request.green_protected_policy
        {
            deny.push("trusted compiled cache write requires green protected policy".into());
        }

        if request.layer == CacheLayer::L5ExplicitSharedCompiledCas
            && !request.explicit_shared_allowed()
        {
            deny.push("CV-LAW-005: cross-project compiled artifact sharing denied without explicit allowlist".into());
        }

        if request.layer.is_compiled()
            && !request.same_repo()
            && request.layer != CacheLayer::L5ExplicitSharedCompiledCas
        {
            deny.push("CV-LAW-005: compiled artifacts are repo-scoped by default".into());
        }

        if request.is_agent_patch
            && request.layer.is_trusted_compiled()
            && matches!(request.action, CacheAction::Write | CacheAction::Promote)
        {
            deny.push("agent patches write quarantine cache only until proof receipt".into());
        }

        if deny.is_empty() {
            allow.push(format!(
                "{} {:?} permitted for {}",
                request.layer.as_str(),
                request.action,
                request.actor_tier
            ));
            if request.layer.is_compiled() {
                allow.push(
                    "compiled cache access remains scoped by key material fingerprint".into(),
                );
            }
            AccessDecision::Allow { reasons: allow }
        } else {
            AccessDecision::Deny { reasons: deny }
        }
    }
}
