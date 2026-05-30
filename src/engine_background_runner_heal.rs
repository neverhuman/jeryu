use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use tracing::{info, warn};

use super::SharedState;
use crate::api::events::TuiEventKind;
use crate::pool;
use crate::state::{Manager, Pool};

const HUNG_MANAGER_STALE_AFTER: chrono::Duration = chrono::Duration::minutes(30);

static LAST_ADVISORY_SIGNATURE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static LAST_LIFECYCLE_STATE: OnceLock<Mutex<RunnerHealLifecycleState>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunnerHealActionKind {
    ScaleUp,
    RestartHung,
    GarbageCollectZombie,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunnerHealAction {
    pub kind: RunnerHealActionKind,
    pub pool_name: String,
    pub detail: String,
    pub target_managers: usize,
    pub active_managers: usize,
    pub live_running_managers: usize,
    pub manager_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunnerHealReport {
    pub generated_at: String,
    pub pool_count: usize,
    pub active_total: usize,
    pub live_total: usize,
    pub action_count: usize,
    pub blocked_reason: Option<String>,
    pub unreachable_nodes: Vec<String>,
    pub signature: String,
    pub actions: Vec<RunnerHealAction>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct RunnerHealLifecycleState {
    last_signature: Option<String>,
}

pub(crate) async fn runner_heal_loop(state: SharedState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;
        if let Err(err) = runner_heal_cycle(&state).await {
            warn!(error = %err, "runner heal preview cycle failed");
        }
    }
}

async fn runner_heal_cycle(state: &SharedState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let managers = state.db.list_managers(None).await?;
    let unreachable_nodes = partial_inventory_nodes().await?;
    let live_by_pool = live_running_by_pool(&state.db, &state.docker, &pools).await?;
    let now = Utc::now();

    let Some(report) = runner_heal_preview_from_snapshot(
        now,
        &pools,
        &managers,
        &live_by_pool,
        &unreachable_nodes,
    ) else {
        return Ok(());
    };

    if !emit_runner_heal_advisory(state, &report).await? {
        info!(
            signature = %report.signature,
            action_count = report.action_count,
            "runner heal preview unchanged; skipped duplicate advisory"
        );
        return Ok(());
    }

    if let Some(blocked_reason) = &report.blocked_reason {
        warn!(
            signature = %report.signature,
            blocked_reason = %blocked_reason,
            unreachable_nodes = ?report.unreachable_nodes,
            "runner heal blocked by partial inventory"
        );
        return Ok(());
    }

    execute_runner_heal_zombie_cleanup(state, &report).await;

    warn!(
        signature = %report.signature,
        action_count = report.action_count,
        active_total = report.active_total,
        live_total = report.live_total,
        "runner heal preview emitted advisory plan"
    );
    Ok(())
}

async fn live_running_by_pool(
    db: &crate::state::Db,
    docker: &crate::docker::DockerCtl,
    pools: &[Pool],
) -> Result<BTreeMap<String, usize>> {
    let mut live_by_pool = BTreeMap::new();
    for pool in pools {
        let live = pool::count_running_managers(db, docker, &pool.name).await?;
        live_by_pool.insert(pool.name.clone(), live as usize);
    }
    Ok(live_by_pool)
}

async fn partial_inventory_nodes() -> Result<Vec<String>> {
    let mut blocked = Vec::new();

    for node in crate::node_support::list_node_configs()? {
        if !node.enabled {
            continue;
        }

        let probe = crate::runner_backend_remote::probe_node(&node).await;
        if !probe.reachable || !probe.docker_ready {
            blocked.push(node.alias);
        }
    }

    Ok(blocked)
}

pub(crate) fn runner_heal_preview_from_snapshot(
    now: DateTime<Utc>,
    pools: &[Pool],
    managers: &[Manager],
    live_by_pool: &BTreeMap<String, usize>,
    unreachable_nodes: &[String],
) -> Option<RunnerHealReport> {
    if !unreachable_nodes.is_empty() {
        let blocked_reason = format!(
            "partial inventory: unreachable nodes {}",
            unreachable_nodes.join(", ")
        );
        let signature = render_signature(&[], Some(&blocked_reason), unreachable_nodes);
        return Some(RunnerHealReport {
            generated_at: now.to_rfc3339(),
            pool_count: pools.len(),
            active_total: managers
                .iter()
                .filter(|manager| manager_state_counts_as_active(&manager.state))
                .count(),
            live_total: live_by_pool.values().copied().sum(),
            action_count: 0,
            blocked_reason: Some(blocked_reason),
            unreachable_nodes: unreachable_nodes.to_vec(),
            signature,
            actions: Vec::new(),
        });
    }

    let mut actions = Vec::new();
    let mut active_total = 0usize;
    let mut live_total = 0usize;

    for pool in pools {
        let target = target_managers(pool);
        let pool_managers: Vec<&Manager> = managers
            .iter()
            .filter(|manager| manager.pool_name == pool.name)
            .collect();
        let active_managers: Vec<&Manager> = pool_managers
            .iter()
            .copied()
            .filter(|manager| manager_state_counts_as_active(&manager.state))
            .collect();
        let active_count = active_managers.len();
        let live_count = live_by_pool.get(&pool.name).copied().unwrap_or_default();
        active_total += active_count;
        live_total += live_count;

        if active_count < target {
            actions.push(RunnerHealAction {
                kind: RunnerHealActionKind::ScaleUp,
                pool_name: pool.name.clone(),
                detail: format!(
                    "pool under target: active={} target={} missing={}",
                    active_count,
                    target,
                    target.saturating_sub(active_count)
                ),
                target_managers: target,
                active_managers: active_count,
                live_running_managers: live_count,
                manager_ids: Vec::new(),
            });
        }

        if live_count > active_count {
            actions.push(RunnerHealAction {
                kind: RunnerHealActionKind::GarbageCollectZombie,
                pool_name: pool.name.clone(),
                detail: format!(
                    "live inventory exceeds DB active rows by {}",
                    live_count - active_count
                ),
                target_managers: target,
                active_managers: active_count,
                live_running_managers: live_count,
                manager_ids: Vec::new(),
            });
        }

        let hung_managers = active_managers
            .iter()
            .filter(|manager| is_hung_manager(manager, now))
            .map(|manager| manager.id.clone())
            .collect::<Vec<_>>();
        if !hung_managers.is_empty() {
            actions.push(RunnerHealAction {
                kind: RunnerHealActionKind::RestartHung,
                pool_name: pool.name.clone(),
                detail: format!(
                    "stale active managers missing fresh contact: {}",
                    hung_managers.join(", ")
                ),
                target_managers: target,
                active_managers: active_count,
                live_running_managers: live_count,
                manager_ids: hung_managers,
            });
        }
    }

    if actions.is_empty() {
        return None;
    }

    let signature = render_signature(&actions, None, unreachable_nodes);
    Some(RunnerHealReport {
        generated_at: now.to_rfc3339(),
        pool_count: pools.len(),
        active_total,
        live_total,
        action_count: actions.len(),
        blocked_reason: None,
        unreachable_nodes: unreachable_nodes.to_vec(),
        signature,
        actions,
    })
}

async fn emit_runner_heal_advisory(state: &SharedState, report: &RunnerHealReport) -> Result<bool> {
    let sig_lock = LAST_ADVISORY_SIGNATURE.get_or_init(|| Mutex::new(None));
    {
        let guard = sig_lock.lock().expect("runner heal advisory lock poisoned");
        if guard.as_deref() == Some(report.signature.as_str()) {
            return Ok(false);
        }
    }

    let payload = serde_json::to_string(report)?;
    state
        .db
        .append_event(
            "runner_heal_preview",
            None,
            None,
            "system_health_loop",
            &payload,
        )
        .await?;

    append_runner_heal_lifecycle_events(state, report).await;

    let mut guard = sig_lock.lock().expect("runner heal advisory lock poisoned");
    *guard = Some(report.signature.clone());
    Ok(true)
}

async fn append_runner_heal_lifecycle_events(state: &SharedState, report: &RunnerHealReport) {
    let maybe_events = {
        let state_lock =
            LAST_LIFECYCLE_STATE.get_or_init(|| Mutex::new(RunnerHealLifecycleState::default()));
        let mut guard = state_lock
            .lock()
            .expect("runner heal lifecycle state poisoned");
        runner_heal_lifecycle_events(&mut guard, report)
    };

    let Some(events) = maybe_events else {
        return;
    };

    for (kind, payload) in events {
        if let Err(err) = state
            .db
            .append_event(
                kind.label(),
                None,
                None,
                "runner_heal_loop",
                &payload.to_string(),
            )
            .await
        {
            warn!(
                event = kind.label(),
                signature = %report.signature,
                error = %err,
                "runner heal lifecycle event append failed"
            );
        }
    }
}

fn runner_heal_lifecycle_events(
    state: &mut RunnerHealLifecycleState,
    report: &RunnerHealReport,
) -> Option<Vec<(TuiEventKind, serde_json::Value)>> {
    if state.last_signature.as_deref() == Some(report.signature.as_str()) {
        return None;
    }
    state.last_signature = Some(report.signature.clone());

    let mut events = Vec::new();
    let orphaned_actions: Vec<_> = report
        .actions
        .iter()
        .filter(|action| matches!(action.kind, RunnerHealActionKind::GarbageCollectZombie))
        .cloned()
        .collect();
    if !orphaned_actions.is_empty() {
        let kind = TuiEventKind::RunnerOrphanedDetected;
        events.push((
            kind,
            json!({
                "kind": kind.label(),
                "signature": report.signature,
                "generated_at": report.generated_at,
                "pool_count": report.pool_count,
                "active_total": report.active_total,
                "live_total": report.live_total,
                "action_count": report.action_count,
                "blocked_reason": report.blocked_reason,
                "unreachable_nodes": report.unreachable_nodes,
                "actions": orphaned_actions,
            }),
        ));
    }

    let hung_actions: Vec<_> = report
        .actions
        .iter()
        .filter(|action| matches!(action.kind, RunnerHealActionKind::RestartHung))
        .cloned()
        .collect();
    if !hung_actions.is_empty() {
        let kind = TuiEventKind::HungRunnerDetected;
        events.push((
            kind,
            json!({
                "kind": kind.label(),
                "signature": report.signature,
                "generated_at": report.generated_at,
                "pool_count": report.pool_count,
                "active_total": report.active_total,
                "live_total": report.live_total,
                "action_count": report.action_count,
                "blocked_reason": report.blocked_reason,
                "unreachable_nodes": report.unreachable_nodes,
                "actions": hung_actions,
            }),
        ));
    }

    Some(events)
}

async fn execute_runner_heal_zombie_cleanup(state: &SharedState, report: &RunnerHealReport) {
    if !report
        .actions
        .iter()
        .any(|action| matches!(action.kind, RunnerHealActionKind::GarbageCollectZombie))
    {
        return;
    }

    let pools = match state.db.list_pools().await {
        Ok(pools) => pools,
        Err(err) => {
            warn!(
                error = %err,
                signature = %report.signature,
                "runner heal zombie cleanup skipped: could not load pools"
            );
            return;
        }
    };

    let mut leases = Vec::with_capacity(pools.len());
    for pool in &pools {
        match crate::pool::PoolOrchestrationLeaseGuard::acquire(&state.db, &pool.name).await {
            Ok(lease) => leases.push(lease),
            Err(err) => {
                warn!(
                    pool = %pool.name,
                    error = %err,
                    signature = %report.signature,
                    "runner heal zombie cleanup skipped: could not acquire pool lease"
                );
                return;
            }
        }
    }

    for pool in &pools {
        if let Err(err) =
            crate::pool::reconcile_manager_runtime_state(&state.db, &state.docker, Some(&pool.name))
                .await
        {
            warn!(
                pool = %pool.name,
                error = %err,
                signature = %report.signature,
                "runner heal zombie cleanup could not refresh runtime state"
            );
        }
    }

    match crate::pool::prune_orphaned_local_runner_containers(&state.db, &state.docker).await {
        Ok(pruned) => {
            let payload = serde_json::json!({
                "signature": report.signature,
                "pruned": pruned,
                "pool_count": report.pool_count,
                "action_count": report.action_count,
                "unreachable_nodes": report.unreachable_nodes,
                "blocked_reason": report.blocked_reason,
            });
            if let Err(err) = state
                .db
                .append_event(
                    "runner_heal_zombie_cleanup",
                    None,
                    None,
                    "system_health_loop",
                    &payload.to_string(),
                )
                .await
            {
                warn!(
                    error = %err,
                    signature = %report.signature,
                    "runner heal zombie cleanup could not append ledger event"
                );
            }
            info!(
                pruned,
                signature = %report.signature,
                "runner heal zombie cleanup executed"
            );
        }
        Err(err) => {
            warn!(
                error = %err,
                signature = %report.signature,
                "runner heal zombie cleanup failed"
            );
        }
    }

    drop(leases);
}

fn target_managers(pool: &Pool) -> usize {
    if pool.name == crate::config::STANDARD_POOL_NAME {
        pool::standard_pool_desired_total()
    } else {
        pool.min_warm.max(0) as usize
    }
}

fn manager_state_counts_as_active(state: &str) -> bool {
    matches!(
        state,
        "starting" | "online" | "node_starting" | "node_unreachable"
    )
}

fn is_hung_manager(manager: &Manager, now: DateTime<Utc>) -> bool {
    if !manager_state_counts_as_active(&manager.state) {
        return false;
    }

    let Some(last_contact_at) = manager.last_contact_at.as_deref() else {
        return false;
    };

    let Ok(last_contact_at) = DateTime::parse_from_rfc3339(last_contact_at) else {
        return false;
    };

    now.signed_duration_since(last_contact_at.with_timezone(&Utc)) >= HUNG_MANAGER_STALE_AFTER
}

fn render_signature(
    actions: &[RunnerHealAction],
    blocked_reason: Option<&str>,
    unreachable_nodes: &[String],
) -> String {
    let blocked = blocked_reason.unwrap_or("");
    let blocked_nodes = unreachable_nodes.join(",");

    format!(
        "blocked={blocked};nodes={blocked_nodes};{}",
        actions
            .iter()
            .map(|action| {
                format!(
                    "{}:{}:{}:{}:{}",
                    action.kind_string(),
                    action.pool_name,
                    action.target_managers,
                    action.active_managers,
                    action.live_running_managers
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    )
}

impl RunnerHealAction {
    fn kind_string(&self) -> &'static str {
        match self.kind {
            RunnerHealActionKind::ScaleUp => "scale_up",
            RunnerHealActionKind::RestartHung => "restart_hung",
            RunnerHealActionKind::GarbageCollectZombie => "garbage_collect_zombie",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use bollard::models::ContainerSummary;

    fn pool(name: &str, min_warm: i64, max_managers: i64) -> Pool {
        Pool {
            name: name.to_string(),
            gitlab_runner_id: 1,
            auth_token: "token".to_string(),
            tags: "tag".to_string(),
            executor: "docker".to_string(),
            min_warm,
            max_managers,
            concurrent: 8,
            request_concurrency: 4,
            paused: false,
            trust_tier: "trusted".to_string(),
            cluster_alias: None,
            backend_type: "docker".to_string(),
        }
    }

    fn manager(pool_name: &str, id: &str, state: &str, last_contact_at: Option<&str>) -> Manager {
        Manager {
            id: id.to_string(),
            pool_name: pool_name.to_string(),
            docker_container_id: format!("container-{id}"),
            system_id: None,
            state: state.to_string(),
            config_dir: "/tmp".to_string(),
            started_at: Some("2026-01-01T00:00:00Z".to_string()),
            last_contact_at: last_contact_at.map(str::to_string),
            node_alias: None,
        }
    }

    fn container(id: &str, name: &str) -> ContainerSummary {
        ContainerSummary {
            id: Some(id.to_string()),
            names: Some(vec![name.to_string()]),
            ..Default::default()
        }
    }

    #[test]
    fn planner_reports_scale_up_restart_and_gc_advisories() {
        let now = DateTime::parse_from_rfc3339("2026-05-29T21:30:00Z")
            .expect("time")
            .with_timezone(&Utc);
        let pools = vec![pool(config::STANDARD_POOL_NAME, 2, 4), pool("docs", 1, 2)];
        let managers = vec![
            manager(
                config::STANDARD_POOL_NAME,
                "mgr-a",
                "online",
                Some("2026-05-29T20:45:00Z"),
            ),
            manager(config::STANDARD_POOL_NAME, "mgr-b", "online", None),
            manager("docs", "mgr-c", "online", Some("2026-05-29T20:00:00Z")),
        ];
        let mut live = BTreeMap::new();
        live.insert(config::STANDARD_POOL_NAME.to_string(), 4);
        live.insert("docs".to_string(), 0);

        let report = runner_heal_preview_from_snapshot(now, &pools, &managers, &live, &[])
            .expect("expected advisory report");

        assert_eq!(report.pool_count, 2);
        assert_eq!(report.active_total, 3);
        assert_eq!(report.live_total, 4);
        assert!(report.blocked_reason.is_none());
        assert!(report.unreachable_nodes.is_empty());
        assert!(report
            .actions
            .iter()
            .any(|action| matches!(action.kind, RunnerHealActionKind::ScaleUp)));
        assert!(report
            .actions
            .iter()
            .any(|action| matches!(action.kind, RunnerHealActionKind::GarbageCollectZombie)));
        assert!(report
            .actions
            .iter()
            .any(|action| matches!(action.kind, RunnerHealActionKind::RestartHung)));
        assert!(report.signature.contains("scale_up"));
        assert!(report.signature.contains("restart_hung"));
        assert!(report.signature.contains("garbage_collect_zombie"));
    }

    #[test]
    fn planner_returns_none_when_fleet_is_balanced_and_fresh() {
        let now = DateTime::parse_from_rfc3339("2026-05-29T21:30:00Z")
            .expect("time")
            .with_timezone(&Utc);
        let pools = vec![pool("docs", 1, 2)];
        let managers = vec![manager(
            "docs",
            "mgr-a",
            "online",
            Some("2026-05-29T21:20:00Z"),
        )];
        let mut live = BTreeMap::new();
        live.insert("docs".to_string(), 1);

        let report = runner_heal_preview_from_snapshot(now, &pools, &managers, &live, &[]);
        assert!(report.is_none());
    }

    #[test]
    fn planner_refuses_partial_inventory() {
        let now = DateTime::parse_from_rfc3339("2026-05-29T21:30:00Z")
            .expect("time")
            .with_timezone(&Utc);
        let pools = vec![pool("docs", 1, 2)];
        let managers = vec![manager(
            "docs",
            "mgr-a",
            "online",
            Some("2026-05-29T21:20:00Z"),
        )];
        let mut live = BTreeMap::new();
        live.insert("docs".to_string(), 1);
        let unreachable_nodes = vec!["xbabe1".to_string(), "xbabe3".to_string()];

        let report =
            runner_heal_preview_from_snapshot(now, &pools, &managers, &live, &unreachable_nodes)
                .expect("expected blocked advisory");

        assert_eq!(report.action_count, 0);
        assert!(report.actions.is_empty());
        assert_eq!(report.unreachable_nodes, unreachable_nodes);
        assert_eq!(
            report.blocked_reason.as_deref(),
            Some("partial inventory: unreachable nodes xbabe1, xbabe3")
        );
        assert!(report.signature.contains("partial inventory"));
    }

    #[test]
    fn runner_heal_zombie_prune_uses_local_orphan_rules_and_still_blocks_partial_inventory() {
        let now = DateTime::parse_from_rfc3339("2026-05-29T21:30:00Z")
            .expect("time")
            .with_timezone(&Utc);
        let pools = vec![pool("docs", 1, 2)];
        let managers = vec![manager(
            "docs",
            "mgr-a",
            "online",
            Some("2026-05-29T21:20:00Z"),
        )];
        let mut live = BTreeMap::new();
        live.insert("docs".to_string(), 2);

        let report = runner_heal_preview_from_snapshot(now, &pools, &managers, &live, &[])
            .expect("expected zombie-prune advisory");

        assert!(report.blocked_reason.is_none());
        assert_eq!(report.action_count, 1);
        assert!(matches!(
            report.actions[0].kind,
            RunnerHealActionKind::GarbageCollectZombie
        ));

        let orphan_managers = vec![
            manager("docs", "active", "online", None),
            manager("docs", "draining", "draining", None),
            manager("docs", "stopped", "stopped", None),
        ];
        let containers = vec![
            container("container-active", "/jeryu-runner-active"),
            container("container-draining", "/jeryu-runner-draining"),
            container("container-stopped", "/jeryu-runner-stopped"),
            container("container-orphan", "/jeryu-runner-orphan"),
            container("container-other", "/other-service"),
        ];

        let orphan_ids =
            crate::pool::orphaned_local_runner_containers(&containers, &orphan_managers)
                .iter()
                .filter_map(|container| container.id.as_deref())
                .collect::<Vec<_>>();
        assert_eq!(orphan_ids, vec!["container-stopped", "container-orphan"]);

        let blocked_report = runner_heal_preview_from_snapshot(
            now,
            &pools,
            &managers,
            &live,
            &["xbabe1".to_string()],
        )
        .expect("expected blocked advisory");

        assert_eq!(blocked_report.action_count, 0);
        assert!(blocked_report.actions.is_empty());
        assert_eq!(
            blocked_report.blocked_reason.as_deref(),
            Some("partial inventory: unreachable nodes xbabe1")
        );
        assert!(blocked_report.signature.contains("partial inventory"));
    }

    fn lifecycle_report(signature: &str, actions: Vec<RunnerHealAction>) -> RunnerHealReport {
        RunnerHealReport {
            generated_at: "2026-05-29T21:30:00Z".to_string(),
            pool_count: 2,
            active_total: 3,
            live_total: 4,
            action_count: actions.len(),
            blocked_reason: None,
            unreachable_nodes: vec!["xbabe1".to_string()],
            signature: signature.to_string(),
            actions,
        }
    }

    #[test]
    fn lifecycle_events_emit_orphaned_and_hung_signals() {
        let report = lifecycle_report(
            "sig-a",
            vec![
                RunnerHealAction {
                    kind: RunnerHealActionKind::GarbageCollectZombie,
                    pool_name: "docs".into(),
                    detail: "live inventory exceeds DB active rows by 2".into(),
                    target_managers: 1,
                    active_managers: 1,
                    live_running_managers: 3,
                    manager_ids: vec!["m1".into(), "m2".into()],
                },
                RunnerHealAction {
                    kind: RunnerHealActionKind::RestartHung,
                    pool_name: "docs".into(),
                    detail: "stale active managers missing fresh contact: m3".into(),
                    target_managers: 1,
                    active_managers: 1,
                    live_running_managers: 3,
                    manager_ids: vec!["m3".into()],
                },
            ],
        );
        let mut state = RunnerHealLifecycleState::default();

        let events =
            runner_heal_lifecycle_events(&mut state, &report).expect("expected lifecycle events");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, TuiEventKind::RunnerOrphanedDetected);
        assert_eq!(events[1].0, TuiEventKind::HungRunnerDetected);
        assert_eq!(
            events[0].1.get("kind").and_then(|value| value.as_str()),
            Some("runner.orphaned.detected")
        );
        assert_eq!(
            events[1].1.get("kind").and_then(|value| value.as_str()),
            Some("runner.hung.detected")
        );
    }

    #[test]
    fn lifecycle_events_dedupe_repeated_signatures() {
        let report = lifecycle_report(
            "sig-b",
            vec![RunnerHealAction {
                kind: RunnerHealActionKind::RestartHung,
                pool_name: "docs".into(),
                detail: "stale active managers missing fresh contact: m3".into(),
                target_managers: 1,
                active_managers: 1,
                live_running_managers: 3,
                manager_ids: vec!["m3".into()],
            }],
        );
        let mut state = RunnerHealLifecycleState::default();

        let first =
            runner_heal_lifecycle_events(&mut state, &report).expect("first signature should emit");
        let second = runner_heal_lifecycle_events(&mut state, &report);

        assert_eq!(first.len(), 1);
        assert!(second.is_none());
    }
}
