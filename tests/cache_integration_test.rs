// Integration tests intentionally hold mocked Mutex guards across awaits to
// serialize against test-local environment state. std::sync::Mutex is the
// right primitive here (cheaper than tokio Mutex for serial test sections).
#![allow(clippy::await_holding_lock)]

use anyhow::Result;
use jeryu::cache_brain::{BuildUnit, BuildUnitType, CacheBrain};
use jeryu::epoch::EpochManager;
use jeryu::explain::{CacheVerdict, MissReason};
use jeryu::policy::TrustTier;
use jeryu::taint::TaintManager;
use sqlx::Row;
use std::os::unix::fs::PermissionsExt;
use std::sync::{LazyLock, Mutex};
use tempfile::TempDir;

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    // SAFETY: this test module serializes environment mutation with ENV_LOCK
    // and restores prior values before releasing the lock.
    unsafe {
        std::env::set_var(key, value);
    }
}

fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
    // SAFETY: this test module serializes environment mutation with ENV_LOCK
    // and restores prior values before releasing the lock.
    unsafe {
        std::env::remove_var(key);
    }
}

/// Create a test DB using the PRODUCTION migration (not test-local schemas).
/// This validates that all queries work against the real schema.
async fn setup_production_db() -> Result<jeryu::state::Db> {
    let db = jeryu::state::Db::open_memory().await?;
    Ok(db)
}

fn make_tool_path(include_sccache: bool) -> Result<TempDir> {
    let dir = TempDir::new()?;
    let cargo = std::process::Command::new("which").arg("cargo").output()?;
    let rustc = std::process::Command::new("which").arg("rustc").output()?;
    let cargo_path = String::from_utf8_lossy(&cargo.stdout).trim().to_string();
    let rustc_path = String::from_utf8_lossy(&rustc.stdout).trim().to_string();
    std::os::unix::fs::symlink(&cargo_path, dir.path().join("cargo"))?;
    std::os::unix::fs::symlink(&rustc_path, dir.path().join("rustc"))?;
    if include_sccache {
        let shim = dir.path().join("sccache");
        std::fs::write(&shim, "#!/bin/sh\nexec \"$@\"\n")?;
        let mut perms = std::fs::metadata(&shim)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&shim, perms)?;
    }
    Ok(dir)
}

fn init_temp_cargo_repo() -> Result<TempDir> {
    let dir = TempDir::new()?;
    let status = std::process::Command::new("cargo")
        .args(["init", "--lib", "--name", "jeryu_local_smoke", "."])
        .current_dir(dir.path())
        .status()?;
    anyhow::ensure!(status.success(), "cargo init failed with {status}");
    Ok(dir)
}

#[tokio::test]
async fn test_cache_brain_rejection_no_namespace() -> Result<()> {
    let db = setup_production_db().await?;
    let epoch_mgr = EpochManager::new(db.pool());
    let taint_mgr = TaintManager::new(db.pool());
    let store = cache_brain_adapter::SqlxActionCacheStore::boxed(
        db.pool(),
        cache_brain_adapter::AdapterBackend::Sqlite,
    );
    let brain = CacheBrain::with_store(epoch_mgr, taint_mgr, store);

    let unit = BuildUnit {
        unit_type: BuildUnitType::GenericStep {
            name: "test".into(),
        },
        input_signature: "invalid_sig_not_in_db".into(),
        environment_signature: "env_sig".into(),
        scope: "project:1".into(),
        trust_tier: TrustTier::Trusted,
    };

    let verdict = brain.plan_step(&unit).await?;

    // Because the DB has no action_cache row for 'invalid_sig_not_in_db', it should Miss (NoLocalCache)
    match verdict {
        CacheVerdict::Miss { reasons } => {
            assert!(
                reasons
                    .iter()
                    .any(|r| matches!(r, MissReason::NoLocalCache))
            );
        }
        _ => panic!("Expected a cache miss due to missing action_cache namespace"),
    }

    Ok(())
}

#[tokio::test]
async fn test_action_cache_roundtrip_hit() -> Result<()> {
    let db = setup_production_db().await?;
    let epoch_mgr = EpochManager::new(db.pool());
    let taint_mgr = TaintManager::new(db.pool());
    let store = cache_brain_adapter::SqlxActionCacheStore::boxed(
        db.pool(),
        cache_brain_adapter::AdapterBackend::Sqlite,
    );
    let brain = CacheBrain::with_store(epoch_mgr, taint_mgr, store);

    let sig = "test_roundtrip_signature_001";

    // Step 1: Verify cold miss
    let unit = BuildUnit {
        unit_type: BuildUnitType::CargoBuild {
            target: "x86_64".into(),
            profile: "release".into(),
            features: "".into(),
        },
        input_signature: sig.into(),
        environment_signature: "".into(),
        scope: "project:42".into(),
        trust_tier: TrustTier::Trusted,
    };
    let verdict = brain.plan_step(&unit).await?;
    assert!(
        matches!(verdict, CacheVerdict::Miss { .. }),
        "Expected cold miss, got {:?}",
        verdict
    );

    // Step 2: Populate action_cache (simulating successful build)
    sqlx::query(
        "INSERT INTO action_cache (action_key, manifest, namespace, created_at) VALUES (?, ?, ?, ?)"
    )
    .bind(sig)
    .bind("{}")
    .bind("trusted")
    .bind("2026-01-01T00:00:00Z")
    .execute(&db.pool())
    .await?;

    // Step 3: Verify hit
    let verdict2 = brain.plan_step(&unit).await?;
    assert!(
        verdict2.is_hit(),
        "Expected HitExact after populating action_cache, got {:?}",
        verdict2
    );

    Ok(())
}

#[tokio::test]
async fn test_taint_propagation_blocks_cache_hit() -> Result<()> {
    let db = setup_production_db().await?;
    let epoch_mgr = EpochManager::new(db.pool());
    let taint_mgr = TaintManager::new(db.pool());
    let store = cache_brain_adapter::SqlxActionCacheStore::boxed(
        db.pool(),
        cache_brain_adapter::AdapterBackend::Sqlite,
    );
    let brain = CacheBrain::with_store(epoch_mgr, taint_mgr.clone(), store);

    let sig = "taint_test_sig_001";

    // Step 1: Populate action_cache
    sqlx::query(
        "INSERT INTO action_cache (action_key, manifest, namespace, created_at) VALUES (?, ?, ?, ?)"
    )
    .bind(sig)
    .bind("{}")
    .bind("trusted")
    .bind("2026-01-01T00:00:00Z")
    .execute(&db.pool())
    .await?;

    // Step 2: Verify hit before taint
    let unit = BuildUnit {
        unit_type: BuildUnitType::GenericStep {
            name: "test".into(),
        },
        input_signature: sig.into(),
        environment_signature: "".into(),
        scope: "project:1".into(),
        trust_tier: TrustTier::Trusted,
    };
    let verdict = brain.plan_step(&unit).await?;
    assert!(verdict.is_hit(), "Expected hit before taint");

    // Step 3: Taint the hash
    taint_mgr
        .propagate_taint(sig, "test poisoning", 999)
        .await?;

    // Step 4: Verify the taint was recorded
    let is_tainted = taint_mgr.is_tainted(sig).await?;
    assert!(is_tainted, "Expected hash to be tainted");

    // Step 5: Verify CacheBrain now denies
    let verdict2 = brain.plan_step(&unit).await?;
    assert!(
        !verdict2.is_hit(),
        "Expected denial/miss after taint, got {:?}",
        verdict2
    );

    Ok(())
}

#[tokio::test]
async fn test_epoch_invalidation() -> Result<()> {
    let db = setup_production_db().await?;
    let epoch_mgr = EpochManager::new(db.pool());

    let scope = "project:99";

    // Step 1: Verify initial epoch is 0
    let initial = epoch_mgr.get_epoch(scope).await?;
    assert_eq!(initial, 0, "Expected 0 for unknown scope");

    // Step 2: Bump epoch
    let new_epoch = epoch_mgr.bump_epoch(scope, 1, "initial setup").await?;
    assert!(new_epoch > 0, "Expected non-zero epoch after bump");

    // Step 3: Verify epoch is valid for current
    let is_valid = epoch_mgr.is_valid(scope, new_epoch).await?;
    assert!(is_valid, "Current epoch should be valid");

    // Step 4: Bump again and verify old epoch invalid
    let newer = epoch_mgr
        .bump_epoch(scope, 2, "poisoned subgraph detected")
        .await?;
    let old_valid = epoch_mgr.is_valid(scope, new_epoch).await?;
    assert!(!old_valid, "Old epoch should be invalid after bump");
    let new_valid = epoch_mgr.is_valid(scope, newer).await?;
    assert!(new_valid, "New epoch should be valid");

    Ok(())
}

#[tokio::test]
async fn test_schema_consistency_taint_queries() -> Result<()> {
    // This test validates that the production migration schema is compatible
    // with all TaintManager queries, catching the class of bug we found.
    let db = setup_production_db().await?;
    let taint_mgr = TaintManager::new(db.pool());

    // Taint an object
    taint_mgr
        .propagate_taint("obj_hash_001", "test reason", 42)
        .await?;

    // Check taint
    let is_tainted = taint_mgr.is_tainted("obj_hash_001").await?;
    assert!(is_tainted);

    // Check non-tainted
    let not_tainted = taint_mgr.is_tainted("obj_hash_999").await?;
    assert!(!not_tainted);

    // Propagate should not error (even if no dependents exist)
    taint_mgr
        .propagate_taint("obj_hash_002", "downstream poison", 42)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_schema_consistency_epoch_queries() -> Result<()> {
    let db = setup_production_db().await?;
    let epoch_mgr = EpochManager::new(db.pool());

    // All epoch operations should work against production schema
    let initial = epoch_mgr.get_epoch("test_scope").await?;
    assert_eq!(initial, 0, "Expected 0 for unknown scope");

    epoch_mgr.bump_epoch("test_scope", 42, "test bump").await?;
    let after = epoch_mgr.get_epoch("test_scope").await?;
    assert!(after > 0, "Expected non-zero epoch after bump");

    Ok(())
}

#[tokio::test]
async fn test_cache_verdicts_recording() -> Result<()> {
    let db = setup_production_db().await?;

    // Simulate recording a verdict (as exec.rs now does)
    sqlx::query(
        "INSERT INTO cache_verdicts (job_id, action_key, object_hash, inputs_hash, verdict, tier, reasons, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(1i64)
    .bind("test_key")
    .bind("test_obj")
    .bind("test_input")
    .bind("Miss")
    .bind("Untrusted")
    .bind("{}")
    .bind("2026-01-01T00:00:00Z")
    .execute(&db.pool())
    .await?;

    // Verify we can query it back
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM cache_verdicts WHERE verdict LIKE '%Miss%'")
            .fetch_one(&db.pool())
            .await?;
    assert_eq!(count, 1);

    // Verify the taint CTE can reference the schema
    let dependents = sqlx::query(
        "WITH RECURSIVE taint_tree AS (
            SELECT object_hash FROM cache_verdicts WHERE inputs_hash = ?
            UNION
            SELECT cv.object_hash FROM cache_verdicts cv
            JOIN taint_tree tt ON cv.inputs_hash = tt.object_hash
        )
        SELECT object_hash FROM taint_tree",
    )
    .bind("test_input")
    .fetch_all(&db.pool())
    .await?;
    assert_eq!(dependents.len(), 1);
    let obj: String = dependents[0].get("object_hash");
    assert_eq!(obj, "test_obj");

    Ok(())
}

#[tokio::test]
async fn test_cache_metrics_integration() -> Result<()> {
    let db = setup_production_db().await?;

    // Record some cache requests
    db.record_cache_request("crates.io/test", "GET", true, "cas_hit", 1024)
        .await?;
    db.record_cache_request("crates.io/test2", "GET", false, "miss", 0)
        .await?;
    db.record_cache_request(
        "crates.io/test3",
        "GET",
        true,
        "singleflight_coalesced",
        2048,
    )
    .await?;

    let metrics = db.get_cache_metrics().await?;
    assert_eq!(metrics.total_requests, 3);
    assert_eq!(metrics.hit_count, 2);
    assert_eq!(metrics.miss_count, 1);
    assert_eq!(metrics.bytes_served, 3072);
    assert!(
        (metrics.hit_ratio - 66.7).abs() < 0.1,
        "Expected ~66.7% hit ratio, got {}",
        metrics.hit_ratio
    );

    Ok(())
}

#[tokio::test]
async fn local_cargo_wrapper_smoke_without_sccache() -> Result<()> {
    let _guard = ENV_LOCK.lock().unwrap();
    let repo = init_temp_cargo_repo()?;
    let tool_path = make_tool_path(false)?;
    let original_path = std::env::var_os("PATH");
    let original_home = std::env::var_os("HOME");
    set_env_var("PATH", tool_path.path());
    set_env_var("HOME", repo.path());
    remove_env_var("JERYU_CARGO_CACHE");

    let result =
        jeryu::local::run_cargo(repo.path().to_path_buf(), vec!["check".to_string()]).await;

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
    match original_home {
        Some(value) => set_env_var("HOME", value),
        None => remove_env_var("HOME"),
    }

    result
}

#[tokio::test]
async fn local_cargo_wrapper_smoke_with_sccache_shim() -> Result<()> {
    let _guard = ENV_LOCK.lock().unwrap();
    let repo = init_temp_cargo_repo()?;
    let tool_path = make_tool_path(true)?;
    let original_path = std::env::var_os("PATH");
    let original_home = std::env::var_os("HOME");
    set_env_var("PATH", tool_path.path());
    set_env_var("HOME", repo.path());
    remove_env_var("JERYU_CARGO_CACHE");

    let result =
        jeryu::local::run_cargo(repo.path().to_path_buf(), vec!["check".to_string()]).await;

    match original_path {
        Some(value) => set_env_var("PATH", value),
        None => remove_env_var("PATH"),
    }
    match original_home {
        Some(value) => set_env_var("HOME", value),
        None => remove_env_var("HOME"),
    }

    result
}

#[tokio::test]
async fn cache_status_report_exposes_local_and_pool_cargo_bytes() -> Result<()> {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_home = TempDir::new()?;
    let original_home = std::env::var_os("HOME");
    set_env_var("HOME", temp_home.path());
    let cache_root = jeryu::config::cache_root_dir();
    let local_target = jeryu::config::local_cargo_targets_root()
        .join("scope")
        .join("rustc")
        .join("host")
        .join("target");
    let local_sccache = jeryu::config::local_cargo_sccache_dir();
    let pool_target = jeryu::config::pool_cargo_targets_root("default")
        .join("scope")
        .join("rustc")
        .join("host")
        .join("target");
    let pool_sccache = jeryu::config::pool_cargo_sccache_dir("default");
    std::fs::create_dir_all(&local_target)?;
    std::fs::create_dir_all(&local_sccache)?;
    std::fs::create_dir_all(&pool_target)?;
    std::fs::create_dir_all(&pool_sccache)?;
    std::fs::write(local_target.join("a"), vec![0_u8; 32])?;
    std::fs::write(local_sccache.join("b"), vec![0_u8; 16])?;
    std::fs::write(pool_target.join("c"), vec![0_u8; 24])?;
    std::fs::write(pool_sccache.join("d"), vec![0_u8; 8])?;

    let db = setup_production_db().await?;
    let report = jeryu::cache::SmartCache::new(db)
        .status_report(None)
        .await?;
    let json = serde_json::to_value(&report)?;
    assert!(report.local_cargo_target_bytes > 0);
    assert!(report.local_cargo_sccache_bytes > 0);
    assert!(report.pool_cargo_target_bytes > 0);
    assert!(report.pool_cargo_sccache_bytes > 0);
    assert!(json.get("local_cargo_target_bytes").is_some());
    assert!(json.get("local_cargo_sccache_bytes").is_some());
    assert!(json.get("pool_cargo_target_bytes").is_some());
    assert!(json.get("pool_cargo_sccache_bytes").is_some());
    assert!(report.jeryu_cache_bytes >= report.local_cargo_target_bytes);
    assert!(cache_root.exists());

    match original_home {
        Some(value) => set_env_var("HOME", value),
        None => remove_env_var("HOME"),
    }
    Ok(())
}
