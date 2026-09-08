use jeryu_cache::cache::{CacheKind, CacheScope};
use jeryu_cache::digest::digest_bytes;
use jeryu_cache::fingerprint::FingerprintInput;
use jeryu_cache::harness::run_adversarial_suite;
use jeryu_cache::ids::{Actor, RepoId, TenantId};
use jeryu_cache::policy::{CacheContext, CachePolicy, TrustTier};
use jeryu_cache::service::{JeryuCache, RestoreStatus, StoreStatus};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("jeryu_cache-{name}-{nanos}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("temp root");
    root
}

fn tenant() -> TenantId {
    TenantId::new("tenant").expect("tenant")
}

fn repo(name: &str) -> RepoId {
    RepoId::new(name).expect("repo")
}

fn actor() -> Actor {
    Actor::new("test").expect("actor")
}

fn scope(tenant: &TenantId, repo: &RepoId) -> CacheScope {
    CacheScope::Repo {
        tenant_id: tenant.clone(),
        repo_id: repo.clone(),
    }
}

#[test]
fn fork_pr_cannot_write_trusted_cache() {
    let root = temp_root("fork-deny");
    let tenant = tenant();
    let repo = repo("repo-a");
    let vault = JeryuCache::open(&root, CachePolicy::new()).expect("vault");
    let ctx = CacheContext::new(tenant.clone(), repo.clone(), TrustTier::ForkPr, false);
    let fp = FingerprintInput::for_repo(tenant.clone(), repo.clone(), TrustTier::ForkPr);
    let out = vault
        .put_cache(
            &ctx,
            actor(),
            CacheKind::CompiledArtifact,
            scope(&tenant, &repo),
            &fp,
            b"artifact",
            true,
        )
        .expect("put");
    assert_eq!(out.status, StoreStatus::Denied);
    assert!(out.entry.is_none());
}

#[test]
fn cross_project_read_denied_by_default() {
    let root = temp_root("cross-project");
    let tenant = tenant();
    let repo_a = repo("repo-a");
    let repo_b = repo("repo-b");
    let vault = JeryuCache::open(&root, CachePolicy::new()).expect("vault");
    let protected = CacheContext::new(
        tenant.clone(),
        repo_a.clone(),
        TrustTier::ProtectedInternal,
        false,
    );
    let fp =
        FingerprintInput::for_repo(tenant.clone(), repo_a.clone(), TrustTier::ProtectedInternal);
    let stored = vault
        .put_cache(
            &protected,
            actor(),
            CacheKind::CompiledArtifact,
            scope(&tenant, &repo_a),
            &fp,
            b"artifact",
            true,
        )
        .expect("put");
    let key = stored.entry.expect("entry").key;
    let cross = CacheContext::new(tenant, repo_b, TrustTier::InternalBranch, false);
    let restored = vault.restore_cache(&cross, actor(), &key).expect("restore");
    assert_eq!(restored.status, RestoreStatus::SafeMiss);
    assert!(restored.bytes.is_none());
}

#[test]
fn build_rs_cache_isolation_changes_key() {
    let tenant = tenant();
    let repo = repo("repo-a");
    let a = FingerprintInput::for_repo(tenant.clone(), repo.clone(), TrustTier::InternalBranch)
        .with_build_rs("build-rs-a", "inputs");
    let b = FingerprintInput::for_repo(tenant, repo, TrustTier::InternalBranch)
        .with_build_rs("build-rs-b", "inputs");
    assert_ne!(a.digest(), b.digest());
}

#[test]
fn proc_macro_cache_isolation_changes_key() {
    let tenant = tenant();
    let repo = repo("repo-a");
    let a = FingerprintInput::for_repo(tenant.clone(), repo.clone(), TrustTier::InternalBranch)
        .with_proc_macro("macro-a");
    let b = FingerprintInput::for_repo(tenant, repo, TrustTier::InternalBranch)
        .with_proc_macro("macro-b");
    assert_ne!(a.digest(), b.digest());
}

#[test]
fn release_lane_ignores_mutable_compiled_cache() {
    let root = temp_root("release-ignore");
    let tenant = tenant();
    let repo = repo("repo-a");
    let vault = JeryuCache::open(&root, CachePolicy::new()).expect("vault");
    let protected = CacheContext::new(
        tenant.clone(),
        repo.clone(),
        TrustTier::ProtectedInternal,
        false,
    );
    let fp = FingerprintInput::for_repo(tenant.clone(), repo.clone(), TrustTier::ProtectedInternal);
    let stored = vault
        .put_cache(
            &protected,
            actor(),
            CacheKind::CompiledArtifact,
            scope(&tenant, &repo),
            &fp,
            b"artifact",
            true,
        )
        .expect("put");
    let release = CacheContext::new(tenant, repo, TrustTier::ReleaseHermetic, true);
    let restored = vault
        .restore_cache(&release, actor(), &stored.entry.expect("entry").key)
        .expect("restore");
    assert_eq!(restored.status, RestoreStatus::SafeMiss);
}

#[test]
fn cache_service_outage_is_safe_miss() {
    let root = temp_root("outage");
    let tenant = tenant();
    let repo = repo("repo-a");
    let vault = JeryuCache::open(&root, CachePolicy::new()).expect("vault");
    let protected = CacheContext::new(
        tenant.clone(),
        repo.clone(),
        TrustTier::ProtectedInternal,
        false,
    );
    let fp = FingerprintInput::for_repo(tenant.clone(), repo.clone(), TrustTier::ProtectedInternal);
    let stored = vault
        .put_cache(
            &protected,
            actor(),
            CacheKind::CompiledArtifact,
            scope(&tenant, &repo),
            &fp,
            b"artifact",
            true,
        )
        .expect("put");
    let key = stored.entry.expect("entry").key;
    vault.cas().set_available(false);
    let restored = vault
        .restore_cache(&protected, actor(), &key)
        .expect("restore");
    assert_eq!(restored.status, RestoreStatus::SafeMiss);
}

#[test]
fn quarantine_then_promote_requires_green_protected_policy() {
    let root = temp_root("promote");
    let tenant = tenant();
    let repo = repo("repo-a");
    let vault = JeryuCache::open(&root, CachePolicy::new()).expect("vault");
    let internal = CacheContext::new(
        tenant.clone(),
        repo.clone(),
        TrustTier::InternalBranch,
        false,
    );
    let protected = CacheContext::new(
        tenant.clone(),
        repo.clone(),
        TrustTier::ProtectedInternal,
        false,
    );
    let fp = FingerprintInput::for_repo(tenant.clone(), repo.clone(), TrustTier::InternalBranch);
    let stored = vault
        .put_cache(
            &internal,
            actor(),
            CacheKind::CompiledArtifact,
            scope(&tenant, &repo),
            &fp,
            b"artifact",
            false,
        )
        .expect("put");
    assert_eq!(stored.status, StoreStatus::Quarantined);
    let key = stored.entry.expect("entry").key;
    let before = vault
        .restore_cache(&internal, actor(), &key)
        .expect("restore");
    assert_eq!(before.status, RestoreStatus::Quarantined);
    let denied = vault
        .promote_cache(&internal, actor(), &key, true)
        .expect("promote");
    assert!(denied.entry.is_none());
    let promoted = vault
        .promote_cache(&protected, actor(), &key, true)
        .expect("promote");
    assert!(promoted.entry.is_some());
    let after = vault
        .restore_cache(&protected, actor(), &key)
        .expect("restore");
    assert_eq!(after.status, RestoreStatus::Hit);
}

#[test]
fn false_hit_detector_quarantines_mismatched_entry() {
    let root = temp_root("false-hit");
    let tenant = tenant();
    let repo = repo("repo-a");
    let vault = JeryuCache::open(&root, CachePolicy::new()).expect("vault");
    let protected = CacheContext::new(
        tenant.clone(),
        repo.clone(),
        TrustTier::ProtectedInternal,
        false,
    );
    let fp = FingerprintInput::for_repo(tenant.clone(), repo.clone(), TrustTier::ProtectedInternal);
    let stored = vault
        .put_cache(
            &protected,
            actor(),
            CacheKind::CompiledArtifact,
            scope(&tenant, &repo),
            &fp,
            b"artifact",
            true,
        )
        .expect("put");
    let key = stored.entry.expect("entry").key;
    let report = vault
        .detect_false_hit(&protected, actor(), &key, &digest_bytes(b"wrong-manifest"))
        .expect("detect");
    assert!(!report.clean);
    let restored = vault
        .restore_cache(&protected, actor(), &key)
        .expect("restore");
    assert_eq!(restored.status, RestoreStatus::Quarantined);
}

#[test]
fn adversarial_harness_passes_all_phase6_laws() {
    let root = temp_root("harness");
    let results = run_adversarial_suite(&root).expect("harness");
    assert!(!results.is_empty());
    for result in results {
        assert!(result.passed, "{}: {}", result.name, result.detail);
    }
}
