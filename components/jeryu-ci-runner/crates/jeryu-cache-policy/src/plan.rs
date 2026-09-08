//! Cache planning and release-safety assertions.

use crate::fingerprint::{FingerprintInputs, fingerprint_job};
use crate::types::{CacheDecision, CachePlan, CachePolicyError, CacheScope};
use jeryu_ci_ir::{Job, TrustTier};

pub fn plan_cache_for_job(job: &Job, trust_tier: &TrustTier) -> CachePlan {
    let mut decisions = Vec::new();
    decisions.push(CacheDecision {
        scope: CacheScope::JobTmpfs,
        read_allowed: true,
        write_allowed: true,
        promote_after_green: false,
        quarantine: false,
        mutable_compiled_cache_allowed: true,
        reason: "job-local tmpfs is ephemeral".to_string(),
    });
    decisions.push(CacheDecision {
        scope: CacheScope::RunnerLocalRegistry,
        read_allowed: true,
        write_allowed: matches!(
            trust_tier,
            TrustTier::ProtectedInternal | TrustTier::InternalBranch
        ),
        promote_after_green: false,
        quarantine: matches!(trust_tier, TrustTier::AgentAuthored),
        mutable_compiled_cache_allowed: false,
        reason: "registry/source cache may be shared, compiled outputs are not implied".to_string(),
    });

    match trust_tier {
        TrustTier::ReleaseHermetic => decisions.push(CacheDecision {
            scope: CacheScope::ReleaseHermeticVendorSnapshot,
            read_allowed: true,
            write_allowed: false,
            promote_after_green: false,
            quarantine: false,
            mutable_compiled_cache_allowed: false,
            reason: "release lanes use locked hermetic inputs only".to_string(),
        }),
        TrustTier::ProtectedInternal | TrustTier::InternalBranch => decisions.push(CacheDecision {
            scope: CacheScope::RepoCompiledCas,
            read_allowed: true,
            write_allowed: true,
            promote_after_green: true,
            quarantine: false,
            mutable_compiled_cache_allowed: true,
            reason: "same repo and trusted tier may read/write project compiled cache after green"
                .to_string(),
        }),
        TrustTier::AgentAuthored => decisions.push(CacheDecision {
            scope: CacheScope::RepoCompiledCas,
            read_allowed: true,
            write_allowed: true,
            promote_after_green: false,
            quarantine: true,
            mutable_compiled_cache_allowed: true,
            reason: "agent writes go to quarantine until proof receipts pass".to_string(),
        }),
        TrustTier::ForkPr | TrustTier::PublicUntrusted => decisions.push(CacheDecision {
            scope: CacheScope::TenantSourceCas,
            read_allowed: true,
            write_allowed: false,
            promote_after_green: false,
            quarantine: false,
            mutable_compiled_cache_allowed: false,
            reason: "untrusted jobs may read source cache but cannot write trusted compiled cache"
                .to_string(),
        }),
    }

    CachePlan {
        job_id: job.id.clone(),
        fingerprint: fingerprint_job(job, trust_tier, &FingerprintInputs::default_for_dev()),
        decisions,
    }
}

pub fn assert_cross_project_compiled_allowed(allow: bool) -> Result<(), CachePolicyError> {
    if allow {
        Ok(())
    } else {
        Err(CachePolicyError::CrossProjectCompiledDenied)
    }
}

pub fn assert_release_cache_safe(plan: &CachePlan) -> Result<(), CachePolicyError> {
    for decision in &plan.decisions {
        let compiled_cache = matches!(
            decision.scope,
            CacheScope::RepoCompiledCas | CacheScope::ExplicitSharedCompiledCas(_)
        );
        if compiled_cache && decision.mutable_compiled_cache_allowed && decision.read_allowed {
            return Err(CachePolicyError::ReleaseMutableCacheDenied);
        }
    }
    Ok(())
}
