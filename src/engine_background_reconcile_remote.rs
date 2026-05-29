//! Owner: Remote-node reconciliation helpers for the engine background loop.
//! Proof: `cargo test -p jeryu -- engine_background`
//! Invariants:
//!   - SSH failures mark managers `node_unreachable`; they never mark them `stopped`.
//!   - Storage GC fires at most once per hour per node (rate-limited by the caller).

use std::time::Instant;
use tracing::{debug, info, warn};

use crate::engine::EngineState;

/// Run storage GC on each registered remote node, rate-limited to once per hour.
pub(super) async fn gc_remote_nodes(state: &EngineState) {
    const GC_INTERVAL_SECS: u64 = 3600; // 1 hour

    let node_configs = match crate::node_support::list_node_configs() {
        Ok(cfgs) => cfgs,
        Err(e) => {
            warn!(error = %e, "failed to load node configs for GC");
            return;
        }
    };

    for cfg in &node_configs {
        if !cfg.enabled {
            continue;
        }

        // Check rate limit.
        let should_gc = {
            let timestamps = state.node_gc_timestamps.lock().unwrap();
            match timestamps.get(&cfg.alias) {
                None => true,
                Some(last) => last.elapsed().as_secs() >= GC_INTERVAL_SECS,
            }
        };

        if !should_gc {
            continue;
        }

        let alias = cfg.alias.clone();
        let limit_gb = cfg.storage_limit_gb;

        if let Some(backend) = state.backend_registry.get_by_alias(&alias) {
            debug!(node = %alias, limit_gb, "running storage GC");
            match backend.gc_storage(limit_gb).await {
                Ok(()) => {
                    let mut timestamps = state.node_gc_timestamps.lock().unwrap();
                    timestamps.insert(alias.clone(), Instant::now());
                }
                Err(e) => {
                    warn!(node = %alias, error = %e, "storage GC failed");
                }
            }
        }
    }
}
