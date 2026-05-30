use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use serde_json::json;
use tracing::warn;

use super::SharedState;
use super::pressure::{handle_nominal_pressure, handle_pressure_cycle};
use crate::api::events::TuiEventKind;

static HEALTH_EVENT_STATE: OnceLock<Mutex<RunnerLifecycleState>> = OnceLock::new();

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct RunnerLifecycleState {
    node_unhealthy: BTreeMap<String, bool>,
    fleet_underfilled: Option<bool>,
    disk_critical: Option<bool>,
}

#[derive(Debug, Clone)]
struct RunnerLifecycleSnapshot {
    observed_at: String,
    nodes: Vec<NodeLifecycleSnapshot>,
    fleet: Option<FleetLifecycleSnapshot>,
    disk_critical: bool,
    root_free_bytes: u64,
    pressure_level: crate::cache::DiskPressureLevel,
}

#[derive(Debug, Clone)]
struct NodeLifecycleSnapshot {
    alias: String,
    reachable: bool,
    docker_ready: bool,
    os: Option<String>,
    arch: Option<String>,
    disk_free_gb: Option<f64>,
}

#[derive(Debug, Clone)]
struct FleetLifecycleSnapshot {
    active_total: usize,
    desired_total: usize,
    live_total: usize,
    pool_name: String,
}

pub(crate) async fn health_cycle(
    state: &SharedState,
    auto_paused_pools: &mut BTreeSet<String>,
    consecutive_zero_freed: &mut u32,
) {
    match crate::cache::df_usage("/").await {
        Ok(fs) => {
            let pressure = crate::cache::root_disk_pressure_level(fs.available_bytes);
            let root_free = fs.available_bytes;
            let root_used = fs.used_percent;
            record_runner_lifecycle_events(state, pressure, root_free).await;

            if pressure == crate::cache::DiskPressureLevel::Nominal {
                handle_nominal_pressure(
                    state,
                    auto_paused_pools,
                    consecutive_zero_freed,
                    root_free,
                    root_used,
                )
                .await;
                return;
            }
            handle_pressure_cycle(
                state,
                auto_paused_pools,
                consecutive_zero_freed,
                pressure,
                root_free,
            )
            .await;
        }
        Err(e) => {
            warn!(error = %e, "failed to check disk usage");
        }
    }
}

async fn record_runner_lifecycle_events(
    state: &SharedState,
    pressure: crate::cache::DiskPressureLevel,
    root_free_bytes: u64,
) {
    let snapshot = match build_runner_lifecycle_snapshot(state, pressure, root_free_bytes).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            warn!(error = %err, "runner lifecycle snapshot failed");
            return;
        }
    };

    let events = {
        let state_lock =
            HEALTH_EVENT_STATE.get_or_init(|| Mutex::new(RunnerLifecycleState::default()));
        let mut guard = state_lock.lock().expect("runner lifecycle state poisoned");
        apply_runner_lifecycle_transitions(&mut guard, &snapshot)
    };

    for (kind, payload) in events {
        if let Err(err) = state
            .db
            .append_event(
                kind.label(),
                None,
                None,
                "system_health_loop",
                &payload.to_string(),
            )
            .await
        {
            warn!(
                event = kind.label(),
                error = %err,
                "runner lifecycle event append failed"
            );
        }
    }
}

async fn build_runner_lifecycle_snapshot(
    state: &SharedState,
    pressure: crate::cache::DiskPressureLevel,
    root_free_bytes: u64,
) -> anyhow::Result<RunnerLifecycleSnapshot> {
    let observed_at = chrono::Utc::now().to_rfc3339();

    let nodes = match crate::node_support::list_node_configs() {
        Ok(configs) => {
            let mut observed = Vec::new();
            for node in configs.into_iter().filter(|node| node.enabled) {
                let probe = crate::runner_backend_remote::probe_node(&node).await;
                observed.push(NodeLifecycleSnapshot {
                    alias: node.alias,
                    reachable: probe.reachable,
                    docker_ready: probe.docker_ready,
                    os: probe.os,
                    arch: probe.arch,
                    disk_free_gb: probe.disk_free_gb,
                });
            }
            observed
        }
        Err(err) => {
            warn!(error = %err, "runner lifecycle node inventory unavailable");
            Vec::new()
        }
    };

    let desired_total = crate::pool::standard_pool_desired_total();
    let live_total = crate::pool::count_running_managers(
        &state.db,
        &state.docker,
        crate::config::STANDARD_POOL_NAME,
    )
    .await? as usize;

    let active_total = state
        .db
        .count_active_managers(crate::config::STANDARD_POOL_NAME)
        .await? as usize;

    let fleet = Some(FleetLifecycleSnapshot {
        active_total,
        desired_total,
        live_total,
        pool_name: crate::config::STANDARD_POOL_NAME.to_string(),
    });

    Ok(RunnerLifecycleSnapshot {
        observed_at,
        nodes,
        fleet,
        disk_critical: matches!(
            pressure,
            crate::cache::DiskPressureLevel::Critical | crate::cache::DiskPressureLevel::Emergency
        ),
        root_free_bytes,
        pressure_level: pressure,
    })
}

fn apply_runner_lifecycle_transitions(
    state: &mut RunnerLifecycleState,
    snapshot: &RunnerLifecycleSnapshot,
) -> Vec<(TuiEventKind, serde_json::Value)> {
    let mut events = Vec::new();

    for node in &snapshot.nodes {
        let healthy = node.reachable && node.docker_ready;
        let current_unhealthy = !healthy;
        let previous = state
            .node_unhealthy
            .insert(node.alias.clone(), current_unhealthy);

        if previous.is_none() || previous == Some(current_unhealthy) {
            continue;
        }

        let kind = if current_unhealthy {
            TuiEventKind::RunnerNodeUnreachable
        } else {
            TuiEventKind::RunnerNodeBackOnline
        };
        events.push((
            kind,
            json!({
                "kind": kind.label(),
                "observed_at": snapshot.observed_at,
                "node_alias": node.alias,
                "reachable": node.reachable,
                "docker_ready": node.docker_ready,
                "healthy": healthy,
                "os": node.os,
                "arch": node.arch,
                "disk_free_gb": node.disk_free_gb,
            }),
        ));
    }

    if let Some(fleet) = &snapshot.fleet {
        let current_underfilled = fleet.live_total < fleet.desired_total;
        let previous = state.fleet_underfilled.replace(current_underfilled);
        if current_underfilled && previous == Some(false) {
            let kind = TuiEventKind::FleetUnderfilled;
            events.push((
                kind,
                json!({
                    "kind": kind.label(),
                    "observed_at": snapshot.observed_at,
                    "pool_name": fleet.pool_name,
                    "active_total": fleet.active_total,
                    "live_total": fleet.live_total,
                    "desired_total": fleet.desired_total,
                    "delta": fleet.desired_total.saturating_sub(fleet.live_total),
                }),
            ));
        }
    }

    let previous = state.disk_critical.replace(snapshot.disk_critical);
    if snapshot.disk_critical && previous == Some(false) {
        let kind = TuiEventKind::RunnerDiskCritical;
        events.push((
            kind,
            json!({
                "kind": kind.label(),
                "observed_at": snapshot.observed_at,
                "root_free_bytes": snapshot.root_free_bytes,
                "root_free_human": crate::cache::human_bytes(snapshot.root_free_bytes),
                "pressure_level": format!("{:?}", snapshot.pressure_level).to_lowercase(),
                "warning_floor_bytes": crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
                "critical_floor_bytes": crate::cache::ROOT_DISK_CRITICAL_MIN_FREE_BYTES,
                "emergency_floor_bytes": crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
            }),
        ));
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(alias: &str, reachable: bool, docker_ready: bool) -> NodeLifecycleSnapshot {
        NodeLifecycleSnapshot {
            alias: alias.to_string(),
            reachable,
            docker_ready,
            os: Some("linux".into()),
            arch: Some("x86_64".into()),
            disk_free_gb: Some(42.0),
        }
    }

    fn snapshot(
        observed_at: &str,
        node: NodeLifecycleSnapshot,
        live_total: usize,
        desired_total: usize,
        pressure_level: crate::cache::DiskPressureLevel,
        root_free_bytes: u64,
    ) -> RunnerLifecycleSnapshot {
        RunnerLifecycleSnapshot {
            observed_at: observed_at.to_string(),
            nodes: vec![node],
            fleet: Some(FleetLifecycleSnapshot {
                active_total: live_total,
                desired_total,
                live_total,
                pool_name: crate::config::STANDARD_POOL_NAME.to_string(),
            }),
            disk_critical: matches!(
                pressure_level,
                crate::cache::DiskPressureLevel::Critical
                    | crate::cache::DiskPressureLevel::Emergency
            ),
            root_free_bytes,
            pressure_level,
        }
    }

    #[test]
    fn lifecycle_transitions_emit_back_online_after_unreachable() {
        let mut state = RunnerLifecycleState::default();
        let baseline = snapshot(
            "2026-05-29T21:00:00Z",
            node("xbabe0", true, true),
            4,
            4,
            crate::cache::DiskPressureLevel::Nominal,
            crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
        );
        let degraded = snapshot(
            "2026-05-29T21:00:00Z",
            node("xbabe0", false, false),
            2,
            4,
            crate::cache::DiskPressureLevel::Critical,
            crate::cache::ROOT_DISK_CRITICAL_MIN_FREE_BYTES - 1,
        );
        let recovered = snapshot(
            "2026-05-29T21:05:00Z",
            node("xbabe0", true, true),
            4,
            4,
            crate::cache::DiskPressureLevel::Nominal,
            crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
        );

        let baseline_events = apply_runner_lifecycle_transitions(&mut state, &baseline);
        let degraded_events = apply_runner_lifecycle_transitions(&mut state, &degraded);
        let recovered_events = apply_runner_lifecycle_transitions(&mut state, &recovered);

        assert!(baseline_events.is_empty());
        assert_eq!(degraded_events.len(), 3);
        assert!(matches!(
            degraded_events[0].0,
            TuiEventKind::RunnerNodeUnreachable
        ));
        assert!(matches!(
            degraded_events[1].0,
            TuiEventKind::FleetUnderfilled
        ));
        assert!(matches!(
            degraded_events[2].0,
            TuiEventKind::RunnerDiskCritical
        ));
        assert_eq!(recovered_events.len(), 1);
        assert!(matches!(
            recovered_events[0].0,
            TuiEventKind::RunnerNodeBackOnline
        ));
    }

    #[test]
    fn lifecycle_transitions_dedupe_repeated_polling() {
        let mut state = RunnerLifecycleState::default();
        let baseline = snapshot(
            "2026-05-29T21:00:00Z",
            node("xbabe0", true, true),
            4,
            4,
            crate::cache::DiskPressureLevel::Nominal,
            crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
        );
        let degraded = snapshot(
            "2026-05-29T21:00:00Z",
            node("xbabe0", false, false),
            2,
            4,
            crate::cache::DiskPressureLevel::Critical,
            crate::cache::ROOT_DISK_CRITICAL_MIN_FREE_BYTES - 1,
        );

        let baseline_events = apply_runner_lifecycle_transitions(&mut state, &baseline);
        let first_events = apply_runner_lifecycle_transitions(&mut state, &degraded);
        let second_events = apply_runner_lifecycle_transitions(&mut state, &degraded);

        assert!(baseline_events.is_empty());
        assert_eq!(first_events.len(), 3);
        assert!(
            first_events
                .iter()
                .any(|event| matches!(event.0, TuiEventKind::RunnerNodeUnreachable))
        );
        assert!(
            first_events
                .iter()
                .any(|event| matches!(event.0, TuiEventKind::FleetUnderfilled))
        );
        assert!(
            first_events
                .iter()
                .any(|event| matches!(event.0, TuiEventKind::RunnerDiskCritical))
        );
        assert!(second_events.is_empty());
    }
}
