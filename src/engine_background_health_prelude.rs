use tracing::{error, warn};

use super::SharedState;

pub(crate) async fn maybe_settle_startup_delay() {
    if let Ok(fs) = crate::cache::df_usage("/").await {
        if fs.available_bytes >= crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        } else {
            warn!(
                root_free = %crate::cache::human_bytes(fs.available_bytes),
                required_free = %crate::cache::human_bytes(
                    crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES
                ),
                "startup pre-flight check detected disk pressure, bypassing settle delay"
            );
        }
    }
}

pub(crate) async fn health_housekeeping(state: &SharedState) {
    let workers_killed = crate::reclaim::gc_orphaned_workers().await;
    if workers_killed > 0 {
        warn!("gc_orphaned_workers: killed {workers_killed} orphaned forkserver processes");
    }

    let mem_gb = crate::reclaim::mem_available_gb();
    if mem_gb < 8.0 {
        error!("CRITICAL memory: {mem_gb:.1}GB available — forcing emergency GC");
        let _ = crate::reclaim::run_auto_gc(&state.docker, true, true).await;
    } else if mem_gb < 15.0 {
        warn!("memory pressure: {mem_gb:.1}GB available — triggering GC");
        let _ = crate::reclaim::run_auto_gc(&state.docker, false, false).await;
    }
}
