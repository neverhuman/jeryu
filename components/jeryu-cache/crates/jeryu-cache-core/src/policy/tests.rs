use super::*;
use crate::tier::TrustTier;

fn base(action: CacheAction, layer: CacheLayer, actor_tier: TrustTier) -> CacheRequest {
    CacheRequest {
        action,
        layer,
        actor_tier,
        source_repo_id: "repo-a".into(),
        target_repo_id: "repo-a".into(),
        scope: CacheScope::Repo {
            tenant_id: "tenant".into(),
            repo_id: "repo-a".into(),
        },
        green_protected_policy: true,
        has_explainable_fingerprint: true,
        has_receipt: true,
        is_release_lane: false,
        is_agent_patch: false,
    }
}

#[test]
fn policy_denies_fork_write_to_trusted_cache() {
    let req = base(
        CacheAction::Write,
        CacheLayer::L3RepoCompiledCas,
        TrustTier::T4ForkPr,
    );
    assert!(!PolicyEngine.evaluate(&req).allowed());
}

#[test]
fn policy_denies_release_mutable_compiled_restore() {
    let mut req = base(
        CacheAction::Restore,
        CacheLayer::L3RepoCompiledCas,
        TrustTier::T0ReleaseHermetic,
    );
    req.is_release_lane = true;
    assert!(!PolicyEngine.evaluate(&req).allowed());
}

#[test]
fn policy_denies_restore_without_fingerprint() {
    let mut req = base(
        CacheAction::Restore,
        CacheLayer::L2RunnerLocalSourceBlob,
        TrustTier::T2InternalBranch,
    );
    req.has_explainable_fingerprint = false;
    assert!(!PolicyEngine.evaluate(&req).allowed());
}

#[test]
fn policy_denies_fork_restore_from_compiled_cache() {
    let req = base(
        CacheAction::Restore,
        CacheLayer::L3RepoCompiledCas,
        TrustTier::T4ForkPr,
    );
    assert!(!PolicyEngine.evaluate(&req).allowed());
}

#[test]
fn policy_allows_fork_restore_from_source_cache() {
    let req = base(
        CacheAction::Restore,
        CacheLayer::L2RunnerLocalSourceBlob,
        TrustTier::T4ForkPr,
    );
    assert!(PolicyEngine.evaluate(&req).allowed());
}

#[test]
fn policy_allows_t1_green_write() {
    let req = base(
        CacheAction::Write,
        CacheLayer::L3RepoCompiledCas,
        TrustTier::T1ProtectedInternal,
    );
    assert!(PolicyEngine.evaluate(&req).allowed());
}

#[test]
fn policy_denies_cross_project_compiled_by_default() {
    let mut req = base(
        CacheAction::Read,
        CacheLayer::L3RepoCompiledCas,
        TrustTier::T2InternalBranch,
    );
    req.target_repo_id = "repo-b".into();
    assert!(!PolicyEngine.evaluate(&req).allowed());
}

#[test]
fn policy_denies_explicit_shared_compiled_without_allowlist_even_same_repo() {
    let mut req = base(
        CacheAction::Read,
        CacheLayer::L5ExplicitSharedCompiledCas,
        TrustTier::T2InternalBranch,
    );
    req.scope = CacheScope::ExplicitShared {
        tenant_id: "tenant".into(),
        scope_id: "shared-compiled".into(),
        allowlisted: false,
    };

    let decision = PolicyEngine.evaluate(&req);

    assert!(!decision.allowed());
    assert!(
        decision
            .reasons()
            .iter()
            .any(|reason| reason.contains("CV-LAW-005"))
    );
}

#[test]
fn policy_allows_explicit_shared_compiled_when_allowlisted() {
    let mut req = base(
        CacheAction::Read,
        CacheLayer::L5ExplicitSharedCompiledCas,
        TrustTier::T2InternalBranch,
    );
    req.scope = CacheScope::ExplicitShared {
        tenant_id: "tenant".into(),
        scope_id: "shared-compiled".into(),
        allowlisted: true,
    };

    assert!(PolicyEngine.evaluate(&req).allowed());
}
