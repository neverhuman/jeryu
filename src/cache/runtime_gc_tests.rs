use super::*;
use tempfile::tempdir;

#[tokio::test]
async fn nominal_pressure_returns_zero_without_touching_anything() {
    let freed = sweep_incremental_caches(DiskPressureLevel::Nominal)
        .await
        .unwrap();
    assert_eq!(freed, 0);
}

#[test]
fn dir_size_counts_files() {
    let tmp = tempdir().unwrap();
    let f = tmp.path().join("a.bin");
    std::fs::write(&f, vec![0u8; 1024]).unwrap();
    assert!(dir_size_bytes(tmp.path()) >= 1024);
}

#[test]
fn has_active_lease_true_when_leases_present() {
    let tmp = tempdir().unwrap();
    let target = tmp.path().join("target");
    let profile = target.join("debug");
    let incremental = profile.join("incremental");
    std::fs::create_dir_all(&incremental).unwrap();
    let leases = target.join(".jeryu").join("leases");
    std::fs::create_dir_all(&leases).unwrap();
    std::fs::write(leases.join("x.json"), "{}").unwrap();
    assert!(has_active_lease(&incremental));
}

#[test]
fn has_active_lease_false_when_no_leases() {
    let tmp = tempdir().unwrap();
    let inc = tmp.path().join("target").join("debug").join("incremental");
    std::fs::create_dir_all(&inc).unwrap();
    assert!(!has_active_lease(&inc));
}
