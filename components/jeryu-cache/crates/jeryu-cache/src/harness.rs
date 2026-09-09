//! Adversarial cache poisoning harness.

use crate::cache::{CacheKind, CacheScope};
use crate::digest::digest_bytes;
use crate::error::Result;
use crate::fingerprint::FingerprintInput;
use crate::ids::{Actor, RepoId, TenantId};
use crate::policy::{CacheContext, CachePolicy, TrustTier};
use crate::service::{JeryuCache, RestoreStatus, StoreStatus};
use std::fs;
use std::path::Path;

/// One harness scenario result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioResult {
    /// Scenario name.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Explanation.
    pub detail: String,
}

impl ScenarioResult {
    fn pass(name: &str, detail: &str) -> Self {
        Self {
            name: name.to_string(),
            passed: true,
            detail: detail.to_string(),
        }
    }

    fn fail(name: &str, detail: &str) -> Self {
        Self {
            name: name.to_string(),
            passed: false,
            detail: detail.to_string(),
        }
    }
}

/// Runs the Phase 6 adversarial suite.
pub fn run_adversarial_suite(root: impl AsRef<Path>) -> Result<Vec<ScenarioResult>> {
    let root = root.as_ref();
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;

    let tenant = TenantId::new("tenant")?;
    let repo_a = RepoId::new("repo-a")?;
    let repo_b = RepoId::new("repo-b")?;
    let actor = Actor::new("harness")?;
    let scope_a = CacheScope::Repo {
        tenant_id: tenant.clone(),
        repo_id: repo_a.clone(),
    };
    let mut results = Vec::new();
    let policy = CachePolicy::new();
    let vault = JeryuCache::open(root, policy)?;
    let protected = CacheContext::new(
        tenant.clone(),
        repo_a.clone(),
        TrustTier::ProtectedInternal,
        false,
    );
    let fork = CacheContext::new(tenant.clone(), repo_a.clone(), TrustTier::ForkPr, false);
    let fp =
        FingerprintInput::for_repo(tenant.clone(), repo_a.clone(), TrustTier::ProtectedInternal);

    let fork_write = vault.put_cache(
        &fork,
        actor.clone(),
        CacheKind::CompiledArtifact,
        scope_a.clone(),
        &fp,
        b"fork bytes",
        true,
    )?;
    results.push(if fork_write.status == StoreStatus::Denied {
        ScenarioResult::pass("fork PR cannot write trusted cache", "write was denied")
    } else {
        ScenarioResult::fail("fork PR cannot write trusted cache", "write was not denied")
    });

    let stored = vault.put_cache(
        &protected,
        actor.clone(),
        CacheKind::CompiledArtifact,
        scope_a.clone(),
        &fp,
        b"trusted bytes",
        true,
    )?;
    let key = stored.entry.expect("trusted write has entry").key;
    let cross_context = CacheContext::new(
        tenant.clone(),
        repo_b.clone(),
        TrustTier::InternalBranch,
        false,
    );
    let cross_read = vault.restore_cache(&cross_context, actor.clone(), &key)?;
    results.push(if cross_read.status == RestoreStatus::SafeMiss {
        ScenarioResult::pass(
            "cross-project read denied by default",
            "read became safe miss",
        )
    } else {
        ScenarioResult::fail(
            "cross-project read denied by default",
            "read was not safe miss",
        )
    });

    let fp_build_a =
        FingerprintInput::for_repo(tenant.clone(), repo_a.clone(), TrustTier::InternalBranch)
            .with_build_rs("build-a", "inputs");
    let fp_build_b =
        FingerprintInput::for_repo(tenant.clone(), repo_a.clone(), TrustTier::InternalBranch)
            .with_build_rs("build-b", "inputs");
    results.push(if fp_build_a.digest() != fp_build_b.digest() {
        ScenarioResult::pass("build.rs cache isolation", "fingerprint changed")
    } else {
        ScenarioResult::fail("build.rs cache isolation", "fingerprint did not change")
    });

    let fp_macro_a =
        FingerprintInput::for_repo(tenant.clone(), repo_a.clone(), TrustTier::InternalBranch)
            .with_proc_macro("macro-a");
    let fp_macro_b =
        FingerprintInput::for_repo(tenant.clone(), repo_a.clone(), TrustTier::InternalBranch)
            .with_proc_macro("macro-b");
    results.push(if fp_macro_a.digest() != fp_macro_b.digest() {
        ScenarioResult::pass("proc-macro cache isolation", "fingerprint changed")
    } else {
        ScenarioResult::fail("proc-macro cache isolation", "fingerprint did not change")
    });

    let release = CacheContext::new(
        tenant.clone(),
        repo_a.clone(),
        TrustTier::ReleaseHermetic,
        true,
    );
    let release_read = vault.restore_cache(&release, actor.clone(), &key)?;
    results.push(if release_read.status == RestoreStatus::SafeMiss {
        ScenarioResult::pass(
            "release ignores mutable cache",
            "release read became safe miss",
        )
    } else {
        ScenarioResult::fail(
            "release ignores mutable cache",
            "release read was not safe miss",
        )
    });

    vault.cas().set_available(false);
    let outage_read = vault.restore_cache(&protected, actor.clone(), &key)?;
    vault.cas().set_available(true);
    results.push(if outage_read.status == RestoreStatus::SafeMiss {
        ScenarioResult::pass(
            "cache service outage safe-miss",
            "outage read became safe miss",
        )
    } else {
        ScenarioResult::fail(
            "cache service outage safe-miss",
            "outage read was not safe miss",
        )
    });

    let wrong_manifest = digest_bytes(b"wrong manifest");
    let report = vault.detect_false_hit(&protected, actor, &key, &wrong_manifest)?;
    results.push(if !report.clean {
        ScenarioResult::pass("false-hit detector", &report.reason)
    } else {
        ScenarioResult::fail("false-hit detector", "mismatch was not detected")
    });

    Ok(results)
}
