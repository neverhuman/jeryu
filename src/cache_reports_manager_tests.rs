use super::*;
use std::sync::{LazyLock, Mutex};
use tempfile::TempDir;

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
    // SAFETY: the test module serializes all environment mutation with ENV_LOCK
    // and the helpers are only used in these single-threaded tests.
    unsafe {
        std::env::set_var(key, value);
    }
}

fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
    // SAFETY: the test module serializes all environment mutation with ENV_LOCK
    // and the helpers are only used in these single-threaded tests.
    unsafe {
        std::env::remove_var(key);
    }
}

#[tokio::test]
async fn test_atomic_store_cas_success() -> Result<()> {
    let data = b"hello world";
    let digest = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    SmartCache::atomic_store_cas(data, digest).await?;

    let cas_dir = crate::config::data_dir().join("cas");
    let path = cas_dir.join(digest);
    assert!(path.exists());

    let _ = tokio::fs::remove_file(path).await;
    Ok(())
}

#[tokio::test]
async fn test_atomic_store_cas_mismatch() {
    let data = b"hello world";
    let bad_digest = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    let result = SmartCache::atomic_store_cas(data, bad_digest).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("mismatched hashes")
    );
}

#[test]
fn gc_path_validation_accepts_literal_special_characters_and_rejects_parent_dirs() {
    let root = Path::new("/tmp/jeryu-cache");
    let good = vec![
        PathBuf::from("/tmp/jeryu-cache/cargo-targets/space dir/target"),
        PathBuf::from("/tmp/jeryu-cache/cargo-targets/quote'\";semi/target"),
        PathBuf::from("/tmp/jeryu-cache/cargo-targets/..literal/target"),
    ];
    let paths = validated_cache_container_paths(root, &good).unwrap();
    assert_eq!(
        paths,
        vec![
            "/cache/cargo-targets/space dir/target".to_string(),
            "/cache/cargo-targets/quote'\";semi/target".to_string(),
            "/cache/cargo-targets/..literal/target".to_string(),
        ]
    );

    let bad = vec![PathBuf::from(
        "/tmp/jeryu-cache/cargo-targets/../escape/target",
    )];
    assert!(validated_cache_container_paths(root, &bad).is_err());
}

#[test]
fn pool_recovery_ttl_uses_override() {
    let _guard = ENV_LOCK.lock().unwrap();
    set_env_var("JERYU_POOL_CARGO_LEASE_RECOVERY_SECS", "7");
    assert_eq!(pool_target_lease_recovery_ttl(), Duration::from_secs(7));
    remove_env_var("JERYU_POOL_CARGO_LEASE_RECOVERY_SECS");
}

#[test]
fn root_disk_pressure_levels_follow_free_space_thresholds() {
    assert_eq!(
        root_disk_pressure_level(ROOT_DISK_WARNING_MIN_FREE_BYTES),
        DiskPressureLevel::Nominal
    );
    assert_eq!(
        root_disk_pressure_level(ROOT_DISK_WARNING_MIN_FREE_BYTES - 1),
        DiskPressureLevel::Warning
    );
    assert_eq!(
        root_disk_pressure_level(ROOT_DISK_CRITICAL_MIN_FREE_BYTES - 1),
        DiskPressureLevel::Critical
    );
    assert_eq!(
        root_disk_pressure_level(ROOT_DISK_EMERGENCY_MIN_FREE_BYTES - 1),
        DiskPressureLevel::Emergency
    );
}

#[tokio::test]
async fn scan_cargo_target_dirs_marks_active_when_any_lease_is_live() -> Result<()> {
    let dir = TempDir::new()?;
    let target = dir
        .path()
        .join("cargo-targets")
        .join("scope")
        .join("target");
    std::fs::create_dir_all(&target)?;
    std::fs::write(target.join("artifact"), b"123")?;
    let lease_dir = target.join(crate::cargo_cache::LEASES_DIR_NAME);
    std::fs::create_dir_all(&lease_dir)?;

    let expired = crate::cargo_cache::CargoLeaseRecord {
        kind: "local-cargo".to_string(),
        scope_key: "scope".to_string(),
        target_dir: target.display().to_string(),
        pid: u32::MAX,
        created_at: chrono::Utc::now().to_rfc3339(),
        rustc_key: "rustc".to_string(),
        rustc_version: "rustc".to_string(),
        host_triple: "host".to_string(),
    };
    let active = crate::cargo_cache::CargoLeaseRecord {
        pid: std::process::id(),
        ..expired.clone()
    };
    std::fs::write(
        lease_dir.join("expired.json"),
        serde_json::to_vec_pretty(&expired)?,
    )?;
    std::fs::write(
        lease_dir.join("active.json"),
        serde_json::to_vec_pretty(&active)?,
    )?;

    let statuses = scan_cargo_target_dirs(&dir.path().join("cargo-targets"), "local").await?;
    assert_eq!(statuses.len(), 1);
    assert!(statuses[0].active);
    assert!(statuses[0].lease_observed);
    assert!(lease_dir.join("active.json").exists());
    assert!(!lease_dir.join("expired.json").exists());
    Ok(())
}

#[tokio::test]
async fn scan_cargo_target_dirs_reports_nested_nextest_extract_scratch() -> Result<()> {
    let dir = TempDir::new()?;
    let target = dir
        .path()
        .join("cargo-targets")
        .join("scope")
        .join("target");
    let nested_extract = target
        .join("nextest")
        .join("extract")
        .join("test-rust-nextest-1");
    std::fs::create_dir_all(&nested_extract)?;
    std::fs::write(nested_extract.join("artifact"), b"nextest")?;

    let statuses = scan_cargo_target_dirs(&dir.path().join("cargo-targets"), "local").await?;
    assert_eq!(statuses.len(), 2);
    assert!(
        statuses
            .iter()
            .any(|status| status.scope == "local" && status.path.ends_with("/target"))
    );
    assert!(statuses.iter().any(|status| {
        status.scope == "local/nextest-extract:test-rust-nextest-1"
            && status.path.ends_with("nextest/extract/test-rust-nextest-1")
    }));
    Ok(())
}
