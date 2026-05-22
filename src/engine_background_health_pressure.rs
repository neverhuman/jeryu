use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{debug, error, info, warn};

use super::SharedState;
use super::pressure_events::{
    record_emergency_pressure_event, record_gc_complete_event, record_gc_pass_event,
    record_gc_stalled_event,
};
use crate::pool;

static GC_RUNNING: AtomicBool = AtomicBool::new(false);

struct GcGuard;

impl Drop for GcGuard {
    fn drop(&mut self) {
        GC_RUNNING.store(false, Ordering::SeqCst);
    }
}

pub(crate) async fn handle_nominal_pressure(
    state: &SharedState,
    auto_paused_pools: &mut BTreeSet<String>,
    consecutive_zero_freed: &mut u32,
    root_free: u64,
    root_used: f64,
) {
    debug!(
        root_free = %crate::cache::human_bytes(root_free),
        root_used = root_used,
        "disk pressure nominal"
    );
    *consecutive_zero_freed = 0;

    if !auto_paused_pools.is_empty() {
        let paused: Vec<String> = auto_paused_pools.iter().cloned().collect();
        for pool_name in paused {
            if let Err(e) = pool::resume_pool(&state.db, &state.client, &pool_name).await {
                error!(
                    error = %e,
                    pool = %pool_name,
                    "failed to resume pool after disk pressure relief"
                );
            } else {
                info!(pool = %pool_name, "resumed pool after disk pressure relief");
                auto_paused_pools.remove(&pool_name);
            }
        }
    }

    let manager = crate::cache::CacheManager;
    if let Err(e) = manager.gc_disk_cache().await {
        error!(error = %e, "background GC failed");
    }
    let _ = tokio::process::Command::new("docker")
        .args(["builder", "prune", "--force", "--filter", "until=2h"])
        .output()
        .await;
}

pub(crate) async fn handle_pressure_cycle(
    state: &SharedState,
    auto_paused_pools: &mut BTreeSet<String>,
    consecutive_zero_freed: &mut u32,
    pressure: crate::cache::DiskPressureLevel,
    root_free: u64,
) {
    let is_critical = matches!(
        pressure,
        crate::cache::DiskPressureLevel::Critical | crate::cache::DiskPressureLevel::Emergency
    );
    let is_emergency = pressure == crate::cache::DiskPressureLevel::Emergency;
    let is_warning = true;

    if GC_RUNNING.swap(true, Ordering::SeqCst) {
        warn!("GC already in progress, skipping this cycle");
        return;
    }

    let _guard = GcGuard;

    if is_emergency {
        warn!(
            root_free = %crate::cache::human_bytes(root_free),
            required_free = %crate::cache::human_bytes(
                crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES
            ),
            "disk pressure emergency: pausing build/default pools and draining managers"
        );

        let pressure_pools = ["build", "default"];
        for pool_name in pressure_pools {
            if auto_paused_pools.contains(pool_name) {
                continue;
            }
            if let Err(e) =
                pool::drain_pool(&state.db, &state.docker, &state.client, pool_name).await
            {
                error!(
                    error = %e,
                    pool = pool_name,
                    "failed to drain pool during disk pressure emergency"
                );
            } else {
                auto_paused_pools.insert(pool_name.to_string());
                info!(pool = pool_name, "drained pool for emergency disk relief");
            }
        }

        record_emergency_pressure_event(state, root_free).await;
    } else {
        warn!(
            root_free = %crate::cache::human_bytes(root_free),
            warning_floor = %crate::cache::human_bytes(
                crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES
            ),
            "disk pressure warning: running cache GC"
        );
    }

    let manager = crate::cache::CacheManager;
    if let Err(e) = manager
        .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
        .await
    {
        error!(error = %e, "cache GC failed");
    }

    let mut current_free = root_free;
    let target_free_bytes = crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES;
    let mut pass = 0u32;
    let mut last_freed_bytes = u64::MAX;
    let usage_before = root_free;

    while current_free < target_free_bytes && pass < 20 {
        pass += 1;
        let escalated = pass > 1 || is_critical;
        let free_before_pass = current_free;

        warn!(
            root_free = %crate::cache::human_bytes(current_free),
            pass,
            critical = escalated,
            emergency = is_emergency,
            "disk pressure: running GC pass"
        );

        record_gc_pass_event(state, current_free, pass, escalated, is_emergency).await;

        if let Err(e) = crate::reclaim::run_auto_gc(&state.docker, escalated, is_emergency).await {
            error!(error = %e, "auto_gc failed");
            break;
        }

        let manager = crate::cache::CacheManager;
        if let Err(e) = manager
            .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
            .await
        {
            error!(error = %e, "cache GC failed");
        }

        match crate::cache::df_usage("/").await {
            Ok(fs) => current_free = fs.available_bytes,
            Err(e) => {
                warn!(error = %e, "failed to refresh disk usage after GC pass");
                break;
            }
        }

        let pass_freed = current_free.saturating_sub(free_before_pass);
        if pass > 2 && pass_freed < 512 * 1024 * 1024 && last_freed_bytes < 512 * 1024 * 1024 {
            warn!(
                pass,
                root_free = %crate::cache::human_bytes(current_free),
                "GC stalled — two consecutive passes freed < 512MiB, stopping early"
            );
            break;
        }
        last_freed_bytes = pass_freed;

        let pass_sleep = if is_emergency {
            10
        } else if is_critical {
            20
        } else {
            30
        };
        tokio::time::sleep(std::time::Duration::from_secs(pass_sleep)).await;
    }

    let freed_bytes = current_free.saturating_sub(usage_before);
    record_gc_complete_event(state, usage_before, current_free, pass).await;

    if freed_bytes == 0 {
        *consecutive_zero_freed += 1;
        if *consecutive_zero_freed >= 3 {
            error!(
                consecutive_stalls = *consecutive_zero_freed,
                root_free = %crate::cache::human_bytes(current_free),
                "disk GC stalled: 3 consecutive cycles freed near-zero space — manual intervention needed"
            );
            record_gc_stalled_event(state, current_free, *consecutive_zero_freed).await;
        }
    } else {
        *consecutive_zero_freed = 0;
        info!(
            freed_bytes,
            root_free_after = %crate::cache::human_bytes(current_free),
            "disk pressure relieved"
        );
    }

    tokio::time::sleep(std::time::Duration::from_secs(120)).await;
}
