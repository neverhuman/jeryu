//! Owner: Interactive TUI subsystem - runtime maintenance loop
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Background maintenance stays bounded, policy-gated, and independent from rendering helpers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{error, warn};

pub(crate) async fn cache_maintenance_loop(docker_ctl: crate::docker::DockerCtl) {
    static GC_RUNNING: AtomicBool = AtomicBool::new(false);

    struct GcGuard;
    impl Drop for GcGuard {
        fn drop(&mut self) {
            GC_RUNNING.store(false, Ordering::SeqCst);
        }
    }

    async fn run_pass(docker_ctl: &crate::docker::DockerCtl) {
        match crate::cache::df_usage("/").await {
            Ok(fs) => {
                let manager = crate::cache::CacheManager;
                let pressure = crate::cache::root_disk_pressure_level(fs.available_bytes);
                let root_free = fs.available_bytes;

                if pressure == crate::cache::DiskPressureLevel::Nominal {
                    if let Err(e) = manager.gc_disk_cache().await {
                        error!(error = %e, "background cache GC failed");
                    }
                    return;
                }

                let is_critical = matches!(
                    pressure,
                    crate::cache::DiskPressureLevel::Critical
                        | crate::cache::DiskPressureLevel::Emergency
                );
                let is_emergency = pressure == crate::cache::DiskPressureLevel::Emergency;
                let is_warning = true;

                if GC_RUNNING.swap(true, Ordering::SeqCst) {
                    warn!("background cache GC already in progress, skipping this cycle");
                    return;
                }

                let _guard = GcGuard;

                if is_emergency {
                    warn!(
                        root_free = %crate::cache::human_bytes(root_free),
                        required_free = %crate::cache::human_bytes(
                            crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES
                        ),
                        "background disk pressure emergency: engine should pause build/default pools and drain managers"
                    );
                } else {
                    warn!(
                        root_free = %crate::cache::human_bytes(root_free),
                        warning_floor = %crate::cache::human_bytes(
                            crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES
                        ),
                        "background disk pressure warning: running cache GC"
                    );
                }

                if let Err(e) =
                    crate::reclaim::run_auto_gc(docker_ctl, is_critical, is_emergency).await
                {
                    error!(error = %e, "background auto_gc failed");
                }

                if let Err(e) = manager
                    .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
                    .await
                {
                    error!(error = %e, "background cache GC failed");
                }
            }
            Err(e) => {
                error!(error = %e, "failed to check disk usage");
            }
        }
    }

    run_pass(&docker_ctl).await;

    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        run_pass(&docker_ctl).await;
    }
}
