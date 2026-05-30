//! Adversarial scenarios for JeryuCache.

use jeryu_cache_core::{
    CacheAction, CacheLayer, CacheRequest, CacheScope, PolicyEngine, TrustTier,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioResult {
    pub name: &'static str,
    pub blocked: bool,
    pub reasons: Vec<String>,
}

pub fn fork_pr_trusted_write() -> ScenarioResult {
    run_scenario(
        "fork-pr-trusted-write",
        CacheRequest {
            action: CacheAction::Write,
            layer: CacheLayer::L3RepoCompiledCas,
            actor_tier: TrustTier::T4ForkPr,
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
        },
    )
}

pub fn cross_project_compiled_read_without_allowlist() -> ScenarioResult {
    run_scenario(
        "cross-project-compiled-read-without-allowlist",
        CacheRequest {
            action: CacheAction::Read,
            layer: CacheLayer::L5ExplicitSharedCompiledCas,
            actor_tier: TrustTier::T2InternalBranch,
            source_repo_id: "repo-a".into(),
            target_repo_id: "repo-b".into(),
            scope: CacheScope::ExplicitShared {
                tenant_id: "tenant".into(),
                scope_id: "shared-rust".into(),
                allowlisted: false,
            },
            green_protected_policy: true,
            has_explainable_fingerprint: true,
            has_receipt: true,
            is_release_lane: false,
            is_agent_patch: false,
        },
    )
}

pub fn release_mutable_compiled_restore() -> ScenarioResult {
    run_scenario(
        "release-mutable-compiled-restore",
        CacheRequest {
            action: CacheAction::Restore,
            layer: CacheLayer::L3RepoCompiledCas,
            actor_tier: TrustTier::T0ReleaseHermetic,
            source_repo_id: "repo-a".into(),
            target_repo_id: "repo-a".into(),
            scope: CacheScope::Repo {
                tenant_id: "tenant".into(),
                repo_id: "repo-a".into(),
            },
            green_protected_policy: true,
            has_explainable_fingerprint: true,
            has_receipt: true,
            is_release_lane: true,
            is_agent_patch: false,
        },
    )
}

pub fn restore_without_fingerprint() -> ScenarioResult {
    run_scenario(
        "restore-without-fingerprint",
        CacheRequest {
            action: CacheAction::Restore,
            layer: CacheLayer::L2RunnerLocalSourceBlob,
            actor_tier: TrustTier::T2InternalBranch,
            source_repo_id: "repo-a".into(),
            target_repo_id: "repo-a".into(),
            scope: CacheScope::Tenant {
                tenant_id: "tenant".into(),
            },
            green_protected_policy: true,
            has_explainable_fingerprint: false,
            has_receipt: true,
            is_release_lane: false,
            is_agent_patch: false,
        },
    )
}

pub fn all_blocking_scenarios() -> Vec<ScenarioResult> {
    vec![
        fork_pr_trusted_write(),
        cross_project_compiled_read_without_allowlist(),
        release_mutable_compiled_restore(),
        restore_without_fingerprint(),
    ]
}

fn run_scenario(name: &'static str, request: CacheRequest) -> ScenarioResult {
    let decision = PolicyEngine.evaluate(&request);
    ScenarioResult {
        name,
        blocked: !decision.allowed(),
        reasons: decision.reasons().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adversarial_cache_laws_block_known_attacks() {
        let results = all_blocking_scenarios();
        for result in results {
            assert!(result.blocked, "{} was not blocked", result.name);
            assert!(
                !result.reasons.is_empty(),
                "{} had no explanation",
                result.name
            );
        }
    }
}
