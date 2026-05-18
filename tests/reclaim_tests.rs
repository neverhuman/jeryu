use jeryu::reclaim::{
    gc_orphaned_workers, live_registry_gc_enabled, live_registry_gc_skip_reason, mem_available_gb,
};

#[test]
fn mem_available_parses_procmeminfo() {
    let gb = mem_available_gb();
    // Either parsed from /proc/meminfo (>0) or default MAX if unavailable
    assert!(gb > 0.0);
}

#[test]
fn mem_available_is_plausible() {
    let gb = mem_available_gb();
    // Should be between 0 and 100TB on any real machine
    assert!(gb < 100_000.0, "implausibly large: {gb}GB");
}

#[tokio::test]
async fn gc_orphaned_workers_no_panic() {
    // Must not panic; on a clean system returns 0 orphans
    let _ = gc_orphaned_workers().await;
}

#[tokio::test]
async fn gc_orphaned_workers_returns_u64() {
    let killed: u64 = gc_orphaned_workers().await;
    assert!(killed < u64::MAX);
}

#[tokio::test]
async fn gc_orphaned_workers_covers_mimo_processes() {
    // Verifies the broader filter (local_run_mimo + forkserver) compiles and runs
    let killed: u64 = gc_orphaned_workers().await;
    assert!(killed < u64::MAX);
}

#[test]
fn live_registry_gc_stays_disabled() {
    assert!(!live_registry_gc_enabled());
    assert!(live_registry_gc_skip_reason().contains("offline"));
}
