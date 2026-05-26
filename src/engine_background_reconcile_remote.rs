//! Owner: Remote-node reconciliation helpers for the engine background loop.
//! Proof: `cargo test -p jeryu -- engine_background`
//! Invariants:
//!   - SSH failures mark managers `node_unreachable`; they never mark them `stopped`.
//!   - Storage GC fires at most once per hour per node (rate-limited by the caller).

use std::collections::BTreeSet;
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

/// Reconcile managers on remote nodes for a pool.
///
/// Groups managers by `node_alias`, makes one SSH call per node to get the
/// running container IDs, then syncs DB states:
/// - Container running      → ensure state is `online` (or `node_starting` → `online`)
/// - Container gone         → set state to `stopped`
/// - SSH unreachable        → set state to `node_unreachable` (don't assume dead)
pub(super) async fn reconcile_remote_managers_for_pool(state: &EngineState, pool_name: &str) {
    let managers = match state.db.list_managers(Some(pool_name)).await {
        Ok(m) => m,
        Err(e) => {
            warn!(pool = pool_name, error = %e, "failed to list managers for remote reconciliation");
            return;
        }
    };

    // Collect unique node aliases for managers in active-ish states.
    let node_aliases: std::collections::HashSet<String> = managers
        .iter()
        .filter(|m| m.node_alias.is_some())
        .filter(|m| {
            matches!(
                m.state.as_str(),
                "starting" | "online" | "node_starting" | "node_unreachable" | "draining"
            )
        })
        .filter_map(|m| m.node_alias.clone())
        .collect();

    for alias in &node_aliases {
        let backend = match state.backend_registry.get_by_alias(alias) {
            Some(b) => b,
            None => {
                warn!(node = %alias, "no backend registered for node alias; skipping reconcile");
                continue;
            }
        };

        let running_ids: BTreeSet<String> = match backend.list_running_backend_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                warn!(node = %alias, error = %e, "SSH failed; marking managers node_unreachable");
                if let Err(db_err) = state.db.mark_node_managers_unreachable(alias).await {
                    warn!(node = %alias, error = %db_err, "failed to mark managers unreachable in DB");
                }
                continue;
            }
        };

        // Sync each manager on this node.
        for m in managers.iter().filter(|m| m.node_alias.as_deref() == Some(alias.as_str())) {
            let is_running = running_ids.contains(&m.docker_container_id);

            match m.state.as_str() {
                "node_starting" | "node_unreachable" if is_running => {
                    info!(
                        manager_id = %m.id,
                        node = %alias,
                        "remote manager recovered and running; marking online"
                    );
                    let _ = state.db.update_manager_state(&m.id, "online").await;
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = state.db.update_manager_last_contact(&m.id, &now).await;
                }
                "online" | "starting" | "node_starting" if !is_running => {
                    warn!(
                        manager_id = %m.id,
                        node = %alias,
                        "remote manager container gone; marking stopped"
                    );
                    let _ = state.db.update_manager_state(&m.id, "stopped").await;
                }
                "online" if is_running => {
                    // Healthy — just refresh last_contact_at.
                    let now = chrono::Utc::now().to_rfc3339();
                    let _ = state.db.update_manager_last_contact(&m.id, &now).await;
                }
                _ => {} // draining, stopped, failed — leave alone
            }
        }
    }
}
