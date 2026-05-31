use super::*;
use crate::cache::{CacheKind, CacheScope};
use crate::ids::{RepoId, TenantId};

fn ctx(tier: TrustTier) -> CacheContext {
    CacheContext::new(
        TenantId::new("tenant").expect("valid tenant"),
        RepoId::new("repo-a").expect("valid repo"),
        tier,
        false,
    )
}

fn repo_scope(name: &str) -> CacheScope {
    CacheScope::Repo {
        tenant_id: TenantId::new("tenant").expect("valid tenant"),
        repo_id: RepoId::new(name).expect("valid repo"),
    }
}

#[test]
fn policy_denies_cross_project_compiled_read() {
    let policy = CachePolicy::new();
    let decision = policy.evaluate(
        &ctx(TrustTier::InternalBranch),
        &repo_scope("repo-b"),
        CacheKind::CompiledArtifact,
        CacheAccess::Read,
    );
    assert_eq!(decision.disposition, CacheDisposition::SafeMiss);
    assert!(!decision.allowed);
}

#[test]
fn policy_denies_fork_compiled_write() {
    let policy = CachePolicy::new();
    let decision = policy.evaluate(
        &ctx(TrustTier::ForkPr),
        &repo_scope("repo-a"),
        CacheKind::CompiledArtifact,
        CacheAccess::Write { ci_green: true },
    );
    assert_eq!(decision.disposition, CacheDisposition::Deny);
    assert!(!decision.allowed);
}
