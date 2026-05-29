//! Owner: Runner Fleet / Inventory Doctor
//! Proof: `cargo test -p jeryu --lib runner_fleet`
//! Invariants: fleet doctor is read-only; repair execution only rehydrates DB
//! rows for running labeled containers and never deletes live containers.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use anyhow::Result;
use futures_util::FutureExt;
use futures_util::future::{BoxFuture, join_all};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use crate::docker::DockerCtl;
use crate::runner_backend::{ManagedContainerSnapshot, RunnerBackend};
use crate::runner_backend_local::LocalDockerBackend;
use crate::runner_backend_remote::RemoteDockerBackend;
use crate::state::{Db, Manager};

pub const DEFAULT_FLEET_PROBE_TIMEOUT_SECS: u64 = 8;

#[derive(Debug, Clone, Copy)]
pub struct RunnerFleetProbeOptions {
    pub node_timeout: Duration,
}

impl Default for RunnerFleetProbeOptions {
    fn default() -> Self {
        Self {
            node_timeout: Duration::from_secs(DEFAULT_FLEET_PROBE_TIMEOUT_SECS),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerFleetReport {
    pub generated_at: String,
    pub ok: bool,
    pub partial: bool,
    pub totals: RunnerFleetTotals,
    pub nodes: Vec<RunnerFleetNodeReport>,
    pub issues: Vec<RunnerFleetIssue>,
    pub repair_preview: Vec<RunnerFleetRepairAction>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunnerFleetTotals {
    pub db_active_managers: usize,
    pub live_running_containers: usize,
    pub containers_seen: usize,
    pub managed: usize,
    pub rehydratable: usize,
    pub orphaned: usize,
    pub stale_created: usize,
    pub missing_label: usize,
    pub over_capacity: usize,
    pub reserved_node_violation: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerFleetNodeReport {
    pub node_alias: String,
    pub reachable: bool,
    pub partial: bool,
    pub error: Option<String>,
    pub reserved: bool,
    pub max_managers: usize,
    pub expected_managers: usize,
    pub db_active_managers: usize,
    pub live_running_containers: usize,
    pub containers: Vec<RunnerFleetContainerReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerFleetContainerReport {
    pub node_alias: String,
    pub container_id: String,
    pub name: Option<String>,
    pub manager_id: Option<String>,
    pub pool_name: Option<String>,
    pub state: String,
    pub running: bool,
    pub classification: RunnerFleetClassification,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerFleetIssue {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub node_alias: Option<String>,
    pub manager_id: Option<String>,
    pub container_id: Option<String>,
    pub repairable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerFleetRepairAction {
    pub action: String,
    pub node_alias: String,
    pub manager_id: Option<String>,
    pub container_id: Option<String>,
    pub reason: String,
    pub destructive: bool,
    pub executed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerFleetRepairReport {
    pub generated_at: String,
    pub executed: bool,
    pub blocked_reason: Option<String>,
    pub actions: Vec<RunnerFleetRepairAction>,
    pub doctor: RunnerFleetReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerFleetClassification {
    Managed,
    Rehydratable,
    Orphaned,
    StaleCreated,
    MissingLabel,
    OverCapacity,
    ReservedNodeViolation,
}

impl RunnerFleetClassification {
    pub fn label(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::Rehydratable => "rehydratable",
            Self::Orphaned => "orphaned",
            Self::StaleCreated => "stale_created",
            Self::MissingLabel => "missing_label",
            Self::OverCapacity => "over_capacity",
            Self::ReservedNodeViolation => "reserved_node_violation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerContainerObservation {
    pub node_alias: String,
    pub container_id: String,
    pub name: Option<String>,
    pub manager_id: Option<String>,
    pub pool_name: Option<String>,
    pub state: String,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerFleetNodePolicy {
    pub node_alias: String,
    pub reserved: bool,
    pub max_managers: usize,
    pub expected_managers: usize,
}

pub async fn build_runner_fleet_report(db: &Db, docker: &DockerCtl) -> Result<RunnerFleetReport> {
    build_runner_fleet_report_with_options(db, docker, RunnerFleetProbeOptions::default()).await
}

pub async fn build_runner_fleet_report_with_options(
    db: &Db,
    docker: &DockerCtl,
    options: RunnerFleetProbeOptions,
) -> Result<RunnerFleetReport> {
    let managers = db.list_managers(None).await?;
    let policies = fleet_node_policies()?;
    let inventories = collect_fleet_inventory(docker, &policies, options).await;
    Ok(classify_runner_fleet(&managers, &policies, inventories))
}

pub async fn repair_runner_fleet(
    db: &Db,
    docker: &DockerCtl,
    yes: bool,
) -> Result<RunnerFleetRepairReport> {
    let mut doctor = build_runner_fleet_report(db, docker).await?;
    let mut actions = doctor.repair_preview.clone();
    let blocked_reason = if yes {
        repair_execution_blocked_reason(&doctor)
    } else {
        None
    };

    if yes && blocked_reason.is_none() {
        for action in &mut actions {
            if action.action != "rehydrate_manager_row" {
                continue;
            }
            let Some(manager_id) = action.manager_id.as_deref() else {
                continue;
            };
            let Some(container_id) = action.container_id.as_deref() else {
                continue;
            };
            let Some(container) = find_container_report(&doctor, manager_id, container_id) else {
                continue;
            };
            let Some(pool_name) = container.pool_name.clone() else {
                continue;
            };
            let config_dir = manager_config_dir_for_action(&action.node_alias, manager_id);
            let manager = Manager {
                id: manager_id.to_string(),
                pool_name,
                docker_container_id: container_id.to_string(),
                system_id: None,
                state: "online".to_string(),
                config_dir,
                started_at: Some(now_rfc3339()),
                last_contact_at: Some(now_rfc3339()),
                node_alias: if action.node_alias == crate::config::LOCAL_NODE_ALIAS {
                    None
                } else {
                    Some(action.node_alias.clone())
                },
            };
            if db.get_manager(manager_id).await?.is_none() {
                db.insert_manager(&manager).await?;
                action.executed = true;
            }
        }
        doctor = build_runner_fleet_report(db, docker).await?;
    }

    Ok(RunnerFleetRepairReport {
        generated_at: now_rfc3339(),
        executed: yes && blocked_reason.is_none(),
        blocked_reason,
        actions,
        doctor,
    })
}

pub fn classify_runner_fleet(
    managers: &[Manager],
    policies: &[RunnerFleetNodePolicy],
    inventories: Vec<NodeInventoryResult>,
) -> RunnerFleetReport {
    let active_managers = managers
        .iter()
        .filter(|manager| manager_state_counts_as_active(&manager.state))
        .collect::<Vec<_>>();
    let active_by_id = active_managers
        .iter()
        .map(|manager| (manager.id.clone(), *manager))
        .collect::<BTreeMap<_, _>>();
    let policies_by_alias = policies
        .iter()
        .map(|policy| (policy.node_alias.clone(), policy.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut nodes = Vec::new();
    let mut issues = Vec::new();

    for inventory in inventories {
        let policy = policies_by_alias
            .get(&inventory.node_alias)
            .cloned()
            .unwrap_or(RunnerFleetNodePolicy {
                node_alias: inventory.node_alias.clone(),
                reserved: false,
                max_managers: usize::MAX,
                expected_managers: 0,
            });
        let node_active = active_managers
            .iter()
            .filter(|manager| manager_node_key(manager) == inventory.node_alias)
            .count();

        if let Some(error) = &inventory.error {
            issues.push(fleet_issue(
                if inventory.timed_out {
                    "node_probe_timeout"
                } else {
                    "node_probe_failed"
                },
                "error",
                format!("{} inventory unavailable: {error}", inventory.node_alias),
                Some(inventory.node_alias.clone()),
                None,
                None,
                true,
            ));
        }

        let mut containers = inventory
            .containers
            .iter()
            .map(|container| classify_container(container, &active_by_id, &policy))
            .collect::<Vec<_>>();
        mark_over_capacity(&policy, &mut containers);

        let live_running = containers
            .iter()
            .filter(|container| container.running)
            .count();
        if node_active != policy.expected_managers && !inventory.partial {
            issues.push(fleet_issue(
                "topology_drift",
                "error",
                format!(
                    "{} has {node_active} active manager row(s), expected {}",
                    inventory.node_alias, policy.expected_managers
                ),
                Some(inventory.node_alias.clone()),
                None,
                None,
                true,
            ));
        }
        if live_running > policy.max_managers {
            issues.push(fleet_issue(
                "over_capacity",
                "error",
                format!(
                    "{} has {live_running} live runner container(s), max {}",
                    inventory.node_alias, policy.max_managers
                ),
                Some(inventory.node_alias.clone()),
                None,
                None,
                true,
            ));
        }
        if policy.reserved && (node_active > 0 || live_running > 0) {
            issues.push(fleet_issue(
                "reserved_node_violation",
                "error",
                format!(
                    "{} is reserved but has {node_active} active manager row(s) and {live_running} live container(s)",
                    inventory.node_alias
                ),
                Some(inventory.node_alias.clone()),
                None,
                None,
                true,
            ));
        }
        for container in &containers {
            match container.classification {
                RunnerFleetClassification::Managed => {}
                RunnerFleetClassification::Rehydratable => issues.push(fleet_issue(
                    "rehydratable_container",
                    "warning",
                    format!(
                        "running container {} has Jeryu labels but no manager row",
                        short_container(&container.container_id)
                    ),
                    Some(container.node_alias.clone()),
                    container.manager_id.clone(),
                    Some(container.container_id.clone()),
                    true,
                )),
                RunnerFleetClassification::Orphaned
                | RunnerFleetClassification::StaleCreated
                | RunnerFleetClassification::MissingLabel => issues.push(fleet_issue(
                    container.classification.label(),
                    "error",
                    container.reason.clone(),
                    Some(container.node_alias.clone()),
                    container.manager_id.clone(),
                    Some(container.container_id.clone()),
                    false,
                )),
                RunnerFleetClassification::OverCapacity
                | RunnerFleetClassification::ReservedNodeViolation => {}
            }
        }

        nodes.push(RunnerFleetNodeReport {
            node_alias: inventory.node_alias,
            reachable: inventory.error.is_none(),
            partial: inventory.partial,
            error: inventory.error,
            reserved: policy.reserved,
            max_managers: policy.max_managers,
            expected_managers: policy.expected_managers,
            db_active_managers: node_active,
            live_running_containers: live_running,
            containers,
        });
    }

    let partial = nodes.iter().any(|node| node.partial);
    let totals = summarize_totals(&nodes);
    let repair_preview = repair_actions_for_nodes(&nodes);
    let ok = !partial && issues.iter().all(|issue| issue.severity != "error");

    RunnerFleetReport {
        generated_at: now_rfc3339(),
        ok,
        partial,
        totals,
        nodes,
        issues,
        repair_preview,
    }
}

#[derive(Debug, Clone)]
pub struct NodeInventoryResult {
    pub node_alias: String,
    pub containers: Vec<RunnerContainerObservation>,
    pub error: Option<String>,
    pub partial: bool,
    pub timed_out: bool,
}

async fn collect_fleet_inventory(
    docker: &DockerCtl,
    policies: &[RunnerFleetNodePolicy],
    options: RunnerFleetProbeOptions,
) -> Vec<NodeInventoryResult> {
    let mut tasks: Vec<BoxFuture<'static, NodeInventoryResult>> = Vec::new();
    let local_alias = crate::config::LOCAL_NODE_ALIAS.to_string();
    let docker = docker.clone();
    tasks.push(
        async move {
            let backend = LocalDockerBackend::new(docker);
            let result = timeout(options.node_timeout, backend.list_managed_containers()).await;
            inventory_result_from_backend(local_alias, result)
        }
        .boxed(),
    );

    let remote_policies = policies
        .iter()
        .filter(|policy| policy.node_alias != crate::config::LOCAL_NODE_ALIAS)
        .map(|policy| policy.node_alias.clone())
        .collect::<BTreeSet<_>>();
    let remote_tasks = remote_policies.into_iter().map(|alias| {
        let timeout_duration = options.node_timeout;
        async move {
            match crate::node_support::load_node_config(&alias) {
                Ok(cfg) if cfg.enabled => {
                    let backend = RemoteDockerBackend::new(cfg);
                    let result = timeout(timeout_duration, backend.list_managed_containers()).await;
                    inventory_result_from_backend(alias, result)
                }
                Ok(_) => NodeInventoryResult {
                    node_alias: alias,
                    containers: Vec::new(),
                    error: Some("node disabled".to_string()),
                    partial: true,
                    timed_out: false,
                },
                Err(err) => NodeInventoryResult {
                    node_alias: alias,
                    containers: Vec::new(),
                    error: Some(err.to_string()),
                    partial: true,
                    timed_out: false,
                },
            }
        }
        .boxed()
    });
    tasks.extend(remote_tasks);
    join_all(tasks).await
}

fn inventory_result_from_backend(
    node_alias: String,
    result: std::result::Result<Result<Vec<ManagedContainerSnapshot>>, tokio::time::error::Elapsed>,
) -> NodeInventoryResult {
    match result {
        Ok(Ok(containers)) => NodeInventoryResult {
            node_alias: node_alias.clone(),
            containers: containers
                .into_iter()
                .map(|container| observation_from_snapshot(&node_alias, container))
                .collect(),
            error: None,
            partial: false,
            timed_out: false,
        },
        Ok(Err(err)) => NodeInventoryResult {
            node_alias,
            containers: Vec::new(),
            error: Some(err.to_string()),
            partial: true,
            timed_out: false,
        },
        Err(_) => NodeInventoryResult {
            node_alias,
            containers: Vec::new(),
            error: Some(format!(
                "probe exceeded {}s deadline",
                DEFAULT_FLEET_PROBE_TIMEOUT_SECS
            )),
            partial: true,
            timed_out: true,
        },
    }
}

fn observation_from_snapshot(
    default_node_alias: &str,
    snapshot: ManagedContainerSnapshot,
) -> RunnerContainerObservation {
    RunnerContainerObservation {
        node_alias: snapshot
            .node_alias
            .unwrap_or_else(|| default_node_alias.to_string()),
        container_id: snapshot.container_id,
        name: Some(format!("jeryu-runner-{}", snapshot.manager_id)),
        manager_id: Some(snapshot.manager_id),
        pool_name: Some(snapshot.pool_name),
        state: snapshot.state,
        running: snapshot.running,
    }
}

fn classify_container(
    container: &RunnerContainerObservation,
    active_by_id: &BTreeMap<String, &Manager>,
    policy: &RunnerFleetNodePolicy,
) -> RunnerFleetContainerReport {
    let classification = if container.manager_id.is_none() || container.pool_name.is_none() {
        RunnerFleetClassification::MissingLabel
    } else if policy.reserved && container.running {
        RunnerFleetClassification::ReservedNodeViolation
    } else if let Some(manager_id) = container.manager_id.as_deref() {
        match active_by_id.get(manager_id) {
            Some(manager) if manager_container_matches(manager, &container.container_id) => {
                RunnerFleetClassification::Managed
            }
            Some(_) => RunnerFleetClassification::Orphaned,
            None if container.running => RunnerFleetClassification::Rehydratable,
            None => RunnerFleetClassification::StaleCreated,
        }
    } else {
        RunnerFleetClassification::MissingLabel
    };
    let reason = classification_reason(classification, container);
    RunnerFleetContainerReport {
        node_alias: container.node_alias.clone(),
        container_id: container.container_id.clone(),
        name: container.name.clone(),
        manager_id: container.manager_id.clone(),
        pool_name: container.pool_name.clone(),
        state: container.state.clone(),
        running: container.running,
        classification,
        reason,
    }
}

fn mark_over_capacity(
    policy: &RunnerFleetNodePolicy,
    containers: &mut [RunnerFleetContainerReport],
) {
    if policy.reserved {
        return;
    }
    let mut running_indices = containers
        .iter()
        .enumerate()
        .filter(|(_, container)| container.running)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    running_indices.sort_by_key(|index| containers[*index].container_id.clone());
    for index in running_indices.into_iter().skip(policy.max_managers) {
        containers[index].classification = RunnerFleetClassification::OverCapacity;
        containers[index].reason = format!(
            "runner container exceeds node max_managers={}",
            policy.max_managers
        );
    }
}

fn fleet_node_policies() -> Result<Vec<RunnerFleetNodePolicy>> {
    let targets = crate::pool::desired_standard_pool_targets();
    let target_by_alias = targets
        .iter()
        .map(|target| (target.node_alias.clone(), target.desired))
        .collect::<BTreeMap<_, _>>();
    let mut aliases = target_by_alias.keys().cloned().collect::<BTreeSet<_>>();
    aliases.extend(
        crate::config::STANDARD_POOL_RESERVED_NODE_ALIASES
            .iter()
            .map(|alias| (*alias).to_string()),
    );
    aliases.insert(crate::config::LOCAL_NODE_ALIAS.to_string());
    for cfg in crate::node_support::list_node_configs()? {
        aliases.insert(cfg.alias);
    }

    let mut policies = Vec::new();
    for alias in aliases {
        let reserved = crate::config::STANDARD_POOL_RESERVED_NODE_ALIASES
            .iter()
            .any(|reserved| *reserved == alias);
        let expected = target_by_alias.get(&alias).copied().unwrap_or(0);
        let max_managers = if alias == crate::config::LOCAL_NODE_ALIAS {
            expected.max(1)
        } else {
            crate::node_support::load_node_config(&alias)
                .map(|cfg| cfg.max_managers)
                .unwrap_or(expected)
        };
        policies.push(RunnerFleetNodePolicy {
            node_alias: alias,
            reserved,
            max_managers,
            expected_managers: expected,
        });
    }
    Ok(policies)
}

fn summarize_totals(nodes: &[RunnerFleetNodeReport]) -> RunnerFleetTotals {
    let mut totals = RunnerFleetTotals {
        db_active_managers: nodes.iter().map(|node| node.db_active_managers).sum(),
        live_running_containers: nodes.iter().map(|node| node.live_running_containers).sum(),
        containers_seen: nodes.iter().map(|node| node.containers.len()).sum(),
        ..RunnerFleetTotals::default()
    };
    for container in nodes.iter().flat_map(|node| node.containers.iter()) {
        match container.classification {
            RunnerFleetClassification::Managed => totals.managed += 1,
            RunnerFleetClassification::Rehydratable => totals.rehydratable += 1,
            RunnerFleetClassification::Orphaned => totals.orphaned += 1,
            RunnerFleetClassification::StaleCreated => totals.stale_created += 1,
            RunnerFleetClassification::MissingLabel => totals.missing_label += 1,
            RunnerFleetClassification::OverCapacity => totals.over_capacity += 1,
            RunnerFleetClassification::ReservedNodeViolation => totals.reserved_node_violation += 1,
        }
    }
    totals
}

fn repair_actions_for_nodes(nodes: &[RunnerFleetNodeReport]) -> Vec<RunnerFleetRepairAction> {
    let mut actions = Vec::new();
    for container in nodes.iter().flat_map(|node| node.containers.iter()) {
        match container.classification {
            RunnerFleetClassification::Rehydratable => actions.push(RunnerFleetRepairAction {
                action: "rehydrate_manager_row".to_string(),
                node_alias: container.node_alias.clone(),
                manager_id: container.manager_id.clone(),
                container_id: Some(container.container_id.clone()),
                reason: "running labeled container is missing from DB".to_string(),
                destructive: false,
                executed: false,
            }),
            RunnerFleetClassification::StaleCreated => actions.push(RunnerFleetRepairAction {
                action: "review_stopped_orphan_container".to_string(),
                node_alias: container.node_alias.clone(),
                manager_id: container.manager_id.clone(),
                container_id: Some(container.container_id.clone()),
                reason: "stopped labeled container is not in DB; GC requires explicit follow-up"
                    .to_string(),
                destructive: false,
                executed: false,
            }),
            RunnerFleetClassification::MissingLabel | RunnerFleetClassification::Orphaned => {
                actions.push(RunnerFleetRepairAction {
                    action: "review_orphan_container".to_string(),
                    node_alias: container.node_alias.clone(),
                    manager_id: container.manager_id.clone(),
                    container_id: Some(container.container_id.clone()),
                    reason: container.reason.clone(),
                    destructive: false,
                    executed: false,
                });
            }
            RunnerFleetClassification::Managed
            | RunnerFleetClassification::OverCapacity
            | RunnerFleetClassification::ReservedNodeViolation => {}
        }
    }
    actions
}

fn repair_execution_blocked_reason(report: &RunnerFleetReport) -> Option<String> {
    if !report.partial {
        return None;
    }
    let partial_nodes = report
        .nodes
        .iter()
        .filter(|node| node.partial)
        .map(|node| node.node_alias.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "fleet inventory is partial for node(s): {partial_nodes}; refusing to execute repair"
    ))
}

fn find_container_report<'a>(
    report: &'a RunnerFleetReport,
    manager_id: &str,
    container_id: &str,
) -> Option<&'a RunnerFleetContainerReport> {
    report
        .nodes
        .iter()
        .flat_map(|node| node.containers.iter())
        .find(|container| {
            container.manager_id.as_deref() == Some(manager_id)
                && container.container_id == container_id
        })
}

fn manager_config_dir_for_action(node_alias: &str, manager_id: &str) -> String {
    if node_alias == crate::config::LOCAL_NODE_ALIAS {
        return crate::config::runners_dir()
            .join(manager_id)
            .display()
            .to_string();
    }
    match crate::node_support::load_node_config(node_alias) {
        Ok(cfg) => format!("{}/{}", cfg.runner_data_dir, manager_id),
        Err(_) => manager_id.to_string(),
    }
}

fn manager_node_key(manager: &Manager) -> String {
    manager
        .node_alias
        .clone()
        .unwrap_or_else(|| crate::config::LOCAL_NODE_ALIAS.to_string())
}

fn manager_state_counts_as_active(state: &str) -> bool {
    matches!(
        state,
        "starting" | "online" | "node_starting" | "node_unreachable" | "draining"
    )
}

fn manager_container_matches(manager: &Manager, container_id: &str) -> bool {
    manager.docker_container_id == container_id
        || manager.docker_container_id.starts_with(container_id)
        || container_id.starts_with(&manager.docker_container_id)
}

fn classification_reason(
    classification: RunnerFleetClassification,
    container: &RunnerContainerObservation,
) -> String {
    match classification {
        RunnerFleetClassification::Managed => {
            "container is backed by an active manager row".to_string()
        }
        RunnerFleetClassification::Rehydratable => {
            "running labeled container can be rehydrated into the DB".to_string()
        }
        RunnerFleetClassification::Orphaned => {
            "container labels reference a DB manager with a different container id".to_string()
        }
        RunnerFleetClassification::StaleCreated => {
            "stopped labeled container has no active manager row".to_string()
        }
        RunnerFleetClassification::MissingLabel => format!(
            "runner-like container {} is missing required Jeryu labels",
            short_container(&container.container_id)
        ),
        RunnerFleetClassification::OverCapacity => "node exceeds configured capacity".to_string(),
        RunnerFleetClassification::ReservedNodeViolation => {
            "container is running on a reserved node".to_string()
        }
    }
}

fn fleet_issue(
    code: &str,
    severity: &str,
    message: String,
    node_alias: Option<String>,
    manager_id: Option<String>,
    container_id: Option<String>,
    repairable: bool,
) -> RunnerFleetIssue {
    RunnerFleetIssue {
        code: code.to_string(),
        severity: severity.to_string(),
        message,
        node_alias,
        manager_id,
        container_id,
        repairable,
    }
}

fn short_container(container_id: &str) -> String {
    container_id.chars().take(12).collect()
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(id: &str, node_alias: Option<&str>, container_id: &str, state: &str) -> Manager {
        Manager {
            id: id.to_string(),
            pool_name: crate::config::STANDARD_POOL_NAME.to_string(),
            docker_container_id: container_id.to_string(),
            system_id: None,
            state: state.to_string(),
            config_dir: format!("/tmp/{id}"),
            started_at: None,
            last_contact_at: None,
            node_alias: node_alias.map(str::to_string),
        }
    }

    fn container(
        id: &str,
        node_alias: &str,
        manager_id: Option<&str>,
        running: bool,
    ) -> RunnerContainerObservation {
        RunnerContainerObservation {
            node_alias: node_alias.to_string(),
            container_id: id.to_string(),
            name: manager_id.map(|id| format!("jeryu-runner-{id}")),
            manager_id: manager_id.map(str::to_string),
            pool_name: manager_id.map(|_| crate::config::STANDARD_POOL_NAME.to_string()),
            state: if running { "running" } else { "exited" }.to_string(),
            running,
        }
    }

    fn policy(alias: &str, max: usize, expected: usize, reserved: bool) -> RunnerFleetNodePolicy {
        RunnerFleetNodePolicy {
            node_alias: alias.to_string(),
            reserved,
            max_managers: max,
            expected_managers: expected,
        }
    }

    #[test]
    fn classification_distinguishes_managed_rehydratable_and_stale() {
        let managers = vec![manager("m1", Some("xbabe0"), "container-1", "online")];
        let inventories = vec![NodeInventoryResult {
            node_alias: "xbabe0".to_string(),
            containers: vec![
                container("container-1", "xbabe0", Some("m1"), true),
                container("container-2", "xbabe0", Some("m2"), true),
                container("container-3", "xbabe0", Some("m3"), false),
            ],
            error: None,
            partial: false,
            timed_out: false,
        }];

        let report =
            classify_runner_fleet(&managers, &[policy("xbabe0", 4, 1, false)], inventories);

        assert_eq!(report.totals.managed, 1);
        assert_eq!(report.totals.rehydratable, 1);
        assert_eq!(report.totals.stale_created, 1);
        assert_eq!(report.repair_preview.len(), 2);
        assert!(!report.ok);
    }

    #[test]
    fn reserved_and_over_capacity_are_failures() {
        let managers = vec![manager("m1", Some("xbabe2"), "c1", "online")];
        let inventories = vec![NodeInventoryResult {
            node_alias: "xbabe2".to_string(),
            containers: vec![container("c1", "xbabe2", Some("m1"), true)],
            error: None,
            partial: false,
            timed_out: false,
        }];

        let report = classify_runner_fleet(&managers, &[policy("xbabe2", 0, 0, true)], inventories);

        assert_eq!(report.totals.reserved_node_violation, 1);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "reserved_node_violation")
        );
        assert!(!report.ok);
    }

    #[test]
    fn timeout_marks_partial_report_unhealthy() {
        let report = classify_runner_fleet(
            &[],
            &[policy("xbabe0", 10, 10, false)],
            vec![NodeInventoryResult {
                node_alias: "xbabe0".to_string(),
                containers: Vec::new(),
                error: Some("probe exceeded 8s deadline".to_string()),
                partial: true,
                timed_out: true,
            }],
        );

        assert!(report.partial);
        assert!(!report.ok);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "node_probe_timeout")
        );
    }

    #[test]
    fn repair_execution_blocks_partial_inventory_even_with_rehydratable_actions() {
        let report = classify_runner_fleet(
            &[],
            &[policy("xbabe0", 10, 10, false)],
            vec![NodeInventoryResult {
                node_alias: "xbabe0".to_string(),
                containers: vec![container("container-2", "xbabe0", Some("m2"), true)],
                error: Some("probe exceeded 8s deadline".to_string()),
                partial: true,
                timed_out: true,
            }],
        );

        assert!(report.partial);
        assert_eq!(report.totals.rehydratable, 1);
        assert!(
            report
                .repair_preview
                .iter()
                .any(|action| action.action == "rehydrate_manager_row")
        );
        assert!(
            repair_execution_blocked_reason(&report)
                .expect("partial report should block execution")
                .contains("xbabe0")
        );
    }
}
