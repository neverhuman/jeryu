//! Owner: Runner Fleet / Pool Doctor
//! Proof: `cargo test -p jeryu -- pool_doctor`
//! Invariants: doctor is read-only; repair does not delete runner records unless requested.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use bollard::models::ContainerSummary;
use serde::Serialize;

use crate::docker::DockerCtl;
use crate::gitlab_client::{GitlabClient, RunnerInfo};
use crate::pool::{self, PoolReservedNode, PoolTopologyPlan};
use crate::runner_backend::RunnerBackend;
use crate::state::{Db, Manager, Pool};

#[derive(Debug, Clone, Serialize)]
pub struct PoolDoctorIssue {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub pool: Option<String>,
    pub runner_id: Option<i64>,
    pub manager_id: Option<String>,
    pub node_alias: Option<String>,
    pub repairable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolDoctorReport {
    pub generated_at: String,
    pub ok: bool,
    pub issues: Vec<PoolDoctorIssue>,
    pub topology: Option<PoolTopologyPlan>,
    pub reserved_nodes: Vec<PoolReservedNode>,
}

#[derive(Debug, Clone, Copy)]
pub struct PoolRepairOptions {
    pub prune_stale: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolRepairReport {
    pub generated_at: String,
    pub actions: Vec<String>,
    pub doctor: PoolDoctorReport,
}

pub async fn build_pool_doctor_report(
    db: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
) -> Result<PoolDoctorReport> {
    let pools = db.list_pools().await?;
    let runners = client.list_all_runner_details().await?;
    let managers = db.list_managers(None).await?;
    let mut issues = Vec::new();

    inspect_standard_pool_policy(&pools, &runners, &mut issues);
    inspect_duplicate_runner_records(&pools, &runners, &mut issues);
    inspect_node_config_policy(&mut issues);
    inspect_manager_runtime(docker, &managers, &mut issues).await;

    let standard_managers = managers
        .iter()
        .filter(|manager| manager.pool_name == crate::config::STANDARD_POOL_NAME)
        .cloned()
        .collect::<Vec<_>>();

    let topology = pools
        .iter()
        .find(|pool| pool.name == crate::config::STANDARD_POOL_NAME)
        .map(|_| {
            pool::plan_pool_topology(
                crate::config::STANDARD_POOL_NAME,
                &standard_managers,
                &pool::desired_standard_pool_targets(),
            )
        });

    if let Some(plan) = &topology {
        for entry in plan
            .entries
            .iter()
            .filter(|entry| entry.active != entry.desired)
        {
            issues.push(issue(
                "topology_drift",
                "error",
                format!(
                    "standard pool has {} active manager(s) on {}, expected {}",
                    entry.active, entry.node_alias, entry.desired
                ),
                Some(crate::config::STANDARD_POOL_NAME.to_string()),
                None,
                None,
                Some(entry.node_alias.clone()),
                true,
            ));
        }
    }

    Ok(PoolDoctorReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        ok: issues.iter().all(|issue| issue.severity != "error"),
        issues,
        topology,
        reserved_nodes: pool::reserved_standard_pool_nodes(&standard_managers),
    })
}

pub async fn repair_pool_state(
    db: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
    options: PoolRepairOptions,
) -> Result<PoolRepairReport> {
    let mut actions = Vec::new();

    let repaired_tags = crate::pool::repair_standard_pool_tags(db).await?;
    if repaired_tags > 0 {
        actions.push(format!(
            "cleared stale DB tags on {repaired_tags} standard pool(s)"
        ));
    }

    let repaired_capacity = crate::pool::repair_standard_pool_capacity(db).await?;
    if repaired_capacity > 0 {
        actions.push("repaired standard pool capacity/backend".to_string());
    }

    for repair in crate::node_support::repair_standard_node_configs()? {
        if repair.changed {
            actions.push(format!(
                "repaired node {}: {}",
                repair.alias,
                repair.changes.join(", ")
            ));
        }
    }

    let pools = db.list_pools().await?;
    let runners = client.list_all_runner_details().await?;
    let runner_by_id = runners
        .iter()
        .map(|runner| (runner.id, runner))
        .collect::<BTreeMap<_, _>>();

    for pool in pools
        .iter()
        .filter(|pool| crate::runner_policy::is_standard_pool_name(&pool.name))
    {
        if pool.paused {
            db.update_pool_paused(&pool.name, false).await?;
            actions.push(format!("resumed DB pool {}", pool.name));
        }
        if let Some(runner) = runner_by_id.get(&pool.gitlab_runner_id) {
            client.update_runner(runner.id, &[], true).await?;
            actions.push(format!(
                "set GitLab runner {} ({}) to untagged",
                runner.id, pool.name
            ));
            if runner.paused.unwrap_or(false) {
                client.set_runner_paused(runner.id, false).await?;
                actions.push(format!("resumed GitLab runner {}", runner.id));
            }
        }
    }

    let pools = db.list_pools().await?;

    if options.prune_stale {
        for runner in stale_standard_runners(&pools, &runners) {
            client.delete_runner(runner.id).await?;
            actions.push(format!("deleted stale GitLab runner {}", runner.id));
        }
    }

    let stopped = crate::pool::reconcile_manager_runtime_state(db, docker, None).await?;
    if stopped > 0 {
        actions.push(format!("marked {stopped} dead manager(s) stopped"));
    }

    if options.prune_stale {
        let pruned = prune_orphaned_local_runner_containers(db, docker).await?;
        if pruned > 0 {
            actions.push(format!(
                "removed {pruned} orphaned local runner container(s)"
            ));
        }
    }

    if pools
        .iter()
        .any(|pool| pool.name == crate::config::STANDARD_POOL_NAME && !pool.paused)
    {
        let started = crate::pool::scale_standard_pool_topology(
            db,
            docker,
            client,
            crate::config::STANDARD_POOL_NAME,
        )
        .await?;
        actions.push(format!(
            "reconciled standard topology; started {started} manager(s)"
        ));
    }

    let doctor = build_pool_doctor_report(db, docker, client).await?;
    Ok(PoolRepairReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        actions,
        doctor,
    })
}

fn inspect_standard_pool_policy(
    pools: &[Pool],
    runners: &[RunnerInfo],
    issues: &mut Vec<PoolDoctorIssue>,
) {
    let runner_by_id = runners
        .iter()
        .map(|runner| (runner.id, runner))
        .collect::<BTreeMap<_, _>>();

    for pool in pools
        .iter()
        .filter(|pool| crate::runner_policy::is_standard_pool_name(&pool.name))
    {
        if !pool.tags.trim().is_empty() {
            issues.push(issue(
                "stale_db_tags",
                "error",
                format!(
                    "standard pool {} has stale DB tags {:?}; standard CI must be untagged",
                    pool.name, pool.tags
                ),
                Some(pool.name.clone()),
                Some(pool.gitlab_runner_id),
                None,
                None,
                true,
            ));
        }
        if pool.paused {
            issues.push(issue(
                "pool_paused",
                "error",
                format!("standard pool {} is paused in the DB", pool.name),
                Some(pool.name.clone()),
                Some(pool.gitlab_runner_id),
                None,
                None,
                true,
            ));
        }
        let desired_capacity = if pool.name == crate::config::STANDARD_POOL_NAME {
            crate::config::STANDARD_POOL_DESIRED_TOTAL
        } else {
            0
        };
        if pool.min_warm != desired_capacity || pool.max_managers != desired_capacity {
            issues.push(issue(
                "standard_pool_capacity_drift",
                "error",
                format!(
                    "standard pool {} has min_warm={} max_managers={}, expected {}",
                    pool.name, pool.min_warm, pool.max_managers, desired_capacity
                ),
                Some(pool.name.clone()),
                Some(pool.gitlab_runner_id),
                None,
                None,
                true,
            ));
        }

        let Some(runner) = runner_by_id.get(&pool.gitlab_runner_id) else {
            issues.push(issue(
                "runner_missing",
                "error",
                format!(
                    "standard pool {} points at missing GitLab runner {}",
                    pool.name, pool.gitlab_runner_id
                ),
                Some(pool.name.clone()),
                Some(pool.gitlab_runner_id),
                None,
                None,
                false,
            ));
            continue;
        };

        if !runner.tag_list.is_empty() || !runner.run_untagged {
            issues.push(issue(
                "gitlab_runner_not_untagged",
                "error",
                format!(
                    "GitLab runner {} for pool {} has tags {:?} and run_untagged={}",
                    runner.id, pool.name, runner.tag_list, runner.run_untagged
                ),
                Some(pool.name.clone()),
                Some(runner.id),
                None,
                None,
                true,
            ));
        }
        if runner.paused.unwrap_or(false) {
            issues.push(issue(
                "gitlab_runner_paused",
                "error",
                format!(
                    "GitLab runner {} for pool {} is paused",
                    runner.id, pool.name
                ),
                Some(pool.name.clone()),
                Some(runner.id),
                None,
                None,
                true,
            ));
        }
    }
}

fn inspect_duplicate_runner_records(
    pools: &[Pool],
    runners: &[RunnerInfo],
    issues: &mut Vec<PoolDoctorIssue>,
) {
    let mut pools_by_runner = BTreeMap::<i64, Vec<&Pool>>::new();
    for pool in pools {
        pools_by_runner
            .entry(pool.gitlab_runner_id)
            .or_default()
            .push(pool);
    }
    for (runner_id, entries) in pools_by_runner
        .iter()
        .filter(|(_, entries)| entries.len() > 1)
    {
        issues.push(issue(
            "duplicate_pool_runner_id",
            "error",
            format!(
                "runner {} is referenced by multiple pools: {}",
                runner_id,
                entries
                    .iter()
                    .map(|pool| pool.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
            Some(*runner_id),
            None,
            None,
            false,
        ));
    }

    let stale = stale_standard_runners(pools, runners);
    for runner in stale {
        issues.push(issue(
            "stale_gitlab_runner",
            "warning",
            format!(
                "GitLab runner {} ({}) is not referenced by any pool",
                runner.id,
                runner.description.as_deref().unwrap_or("-")
            ),
            None,
            Some(runner.id),
            None,
            None,
            true,
        ));
    }
}

fn inspect_node_config_policy(issues: &mut Vec<PoolDoctorIssue>) {
    let expected_gitlab_url = crate::node_support::standard_gitlab_url_override();
    for alias in crate::config::STANDARD_POOL_REMOTE_NODE_ALIASES {
        let cfg = match crate::node_support::load_node_config(alias) {
            Ok(cfg) => cfg,
            Err(err) => {
                issues.push(issue(
                    "node_config_missing",
                    "error",
                    format!("standard node {alias} config is missing or unreadable: {err}"),
                    Some(crate::config::STANDARD_POOL_NAME.to_string()),
                    None,
                    None,
                    Some((*alias).to_string()),
                    true,
                ));
                continue;
            }
        };
        if !cfg.enabled {
            issues.push(issue(
                "node_disabled",
                "error",
                format!("standard node {alias} is disabled"),
                Some(crate::config::STANDARD_POOL_NAME.to_string()),
                None,
                None,
                Some((*alias).to_string()),
                true,
            ));
        }
        if cfg.max_managers != 10 {
            issues.push(issue(
                "node_capacity_drift",
                "error",
                format!(
                    "standard node {alias} has max_managers={}, expected 10",
                    cfg.max_managers
                ),
                Some(crate::config::STANDARD_POOL_NAME.to_string()),
                None,
                None,
                Some((*alias).to_string()),
                true,
            ));
        }
        if !cfg.runner_data_dir.starts_with('/') || !cfg.runner_cache_dir.starts_with('/') {
            issues.push(issue(
                "node_relative_runner_dirs",
                "error",
                format!("standard node {alias} runner/cache dirs must be absolute"),
                Some(crate::config::STANDARD_POOL_NAME.to_string()),
                None,
                None,
                Some((*alias).to_string()),
                true,
            ));
        }
        if let Some(expected) = &expected_gitlab_url
            && cfg.gitlab_url_override.as_deref() != Some(expected.as_str())
        {
            issues.push(issue(
                "node_gitlab_url_drift",
                "error",
                format!("standard node {alias} does not use the shared GitLab URL override"),
                Some(crate::config::STANDARD_POOL_NAME.to_string()),
                None,
                None,
                Some((*alias).to_string()),
                true,
            ));
        }
    }

    for alias in crate::config::STANDARD_POOL_RESERVED_NODE_ALIASES {
        if let Ok(cfg) = crate::node_support::load_node_config(alias)
            && cfg.enabled
            && cfg.accepts_pool(crate::config::STANDARD_POOL_NAME)
        {
            issues.push(issue(
                "reserved_node_excluded",
                "info",
                format!(
                    "{alias} is healthy/registered but reserved for active agent execution; standard pool desired managers=0"
                ),
                Some(crate::config::STANDARD_POOL_NAME.to_string()),
                None,
                None,
                Some((*alias).to_string()),
                false,
            ));
        }
    }
}

async fn inspect_manager_runtime(
    docker: &DockerCtl,
    managers: &[Manager],
    issues: &mut Vec<PoolDoctorIssue>,
) {
    let running_local = docker
        .running_managed_container_ids()
        .await
        .unwrap_or_default();
    for manager in managers
        .iter()
        .filter(|manager| manager_state_counts_as_active(&manager.state))
        .filter(|manager| manager.node_alias.is_none())
        .filter(|manager| !container_ids_match(&running_local, &manager.docker_container_id))
    {
        issues.push(issue(
            "dead_manager",
            "error",
            format!(
                "local manager {} for pool {} is active in DB but not running",
                manager.id, manager.pool_name
            ),
            Some(manager.pool_name.clone()),
            None,
            Some(manager.id.clone()),
            Some(crate::config::LOCAL_NODE_ALIAS.to_string()),
            true,
        ));
    }

    if let Ok(containers) = docker.list_managed_containers().await {
        for container in orphaned_local_runner_containers(&containers, managers) {
            let name = container_summary_name(container);
            issues.push(issue(
                "orphaned_local_container",
                "error",
                format!("local runner container {name} is not attached to an active manager row"),
                None,
                None,
                None,
                Some(crate::config::LOCAL_NODE_ALIAS.to_string()),
                true,
            ));
        }
    }

    let aliases = managers
        .iter()
        .filter(|manager| manager_state_counts_as_active(&manager.state))
        .filter_map(|manager| manager.node_alias.clone())
        .collect::<BTreeSet<_>>();
    for alias in aliases {
        let cfg = match crate::node_support::load_node_config(&alias) {
            Ok(cfg) => cfg,
            Err(err) => {
                issues.push(issue(
                    "node_config_missing",
                    "error",
                    format!("manager node {alias} config is missing: {err}"),
                    None,
                    None,
                    None,
                    Some(alias),
                    true,
                ));
                continue;
            }
        };
        let backend = crate::runner_backend_remote::RemoteDockerBackend::new(cfg);
        let running = match backend.list_running_backend_ids().await {
            Ok(running) => running,
            Err(err) => {
                issues.push(issue(
                    "node_unreachable",
                    "error",
                    format!("could not inspect node {alias}: {err}"),
                    None,
                    None,
                    None,
                    Some(alias),
                    true,
                ));
                continue;
            }
        };
        for manager in managers
            .iter()
            .filter(|manager| manager.node_alias.as_deref() == Some(alias.as_str()))
            .filter(|manager| manager_state_counts_as_active(&manager.state))
            .filter(|manager| !container_ids_match(&running, &manager.docker_container_id))
        {
            issues.push(issue(
                "dead_manager",
                "error",
                format!(
                    "remote manager {} for pool {} is active in DB but not running",
                    manager.id, manager.pool_name
                ),
                Some(manager.pool_name.clone()),
                None,
                Some(manager.id.clone()),
                Some(alias.clone()),
                true,
            ));
        }
    }
}

fn stale_standard_runners<'a>(pools: &[Pool], runners: &'a [RunnerInfo]) -> Vec<&'a RunnerInfo> {
    let referenced = pools
        .iter()
        .map(|pool| pool.gitlab_runner_id)
        .collect::<BTreeSet<_>>();
    let standard_descriptions = crate::config::DEFAULT_POOLS
        .iter()
        .map(|pool| format!("jeryu-{}", pool.name))
        .collect::<BTreeSet<_>>();

    runners
        .iter()
        .filter(|runner| !referenced.contains(&runner.id))
        .filter(|runner| {
            runner
                .description
                .as_ref()
                .is_some_and(|description| standard_descriptions.contains(description))
        })
        .collect()
}

async fn prune_orphaned_local_runner_containers(db: &Db, docker: &DockerCtl) -> Result<usize> {
    let managers = db.list_managers(None).await?;
    let containers = docker.list_managed_containers().await?;
    let orphans = orphaned_local_runner_containers(&containers, &managers);
    let mut pruned = 0;

    for container in orphans {
        if let Some(container_id) = &container.id {
            docker.remove_runner_manager(container_id).await?;
            pruned += 1;
        }
    }

    Ok(pruned)
}

fn orphaned_local_runner_containers<'a>(
    containers: &'a [ContainerSummary],
    managers: &[Manager],
) -> Vec<&'a ContainerSummary> {
    let retained_ids = managers
        .iter()
        .filter(|manager| manager.node_alias.is_none())
        .filter(|manager| manager_state_retains_container(&manager.state))
        .map(|manager| manager.docker_container_id.clone())
        .collect::<BTreeSet<_>>();

    containers
        .iter()
        .filter(|container| {
            container
                .names
                .as_ref()
                .is_some_and(|names| names.iter().any(|name| name.contains("jeryu-runner-")))
        })
        .filter(|container| {
            container
                .id
                .as_deref()
                .is_some_and(|id| !container_ids_match(&retained_ids, id))
        })
        .collect()
}

fn manager_state_retains_container(state: &str) -> bool {
    manager_state_counts_as_active(state) || state == "draining"
}

fn container_summary_name(container: &ContainerSummary) -> String {
    container
        .names
        .as_ref()
        .and_then(|names| names.first())
        .cloned()
        .unwrap_or_else(|| container.id.clone().unwrap_or_else(|| "?".to_string()))
}

fn manager_state_counts_as_active(state: &str) -> bool {
    matches!(
        state,
        "starting" | "online" | "node_starting" | "node_unreachable"
    )
}

fn container_ids_match(running: &BTreeSet<String>, expected: &str) -> bool {
    running.iter().any(|container_id| {
        container_id == expected
            || container_id.starts_with(expected)
            || expected.starts_with(container_id)
    })
}

#[allow(clippy::too_many_arguments)]
fn issue(
    code: &str,
    severity: &str,
    message: String,
    pool: Option<String>,
    runner_id: Option<i64>,
    manager_id: Option<String>,
    node_alias: Option<String>,
    repairable: bool,
) -> PoolDoctorIssue {
    PoolDoctorIssue {
        code: code.to_string(),
        severity: severity.to_string(),
        message,
        pool,
        runner_id,
        manager_id,
        node_alias,
        repairable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(name: &str, runner_id: i64, tags: &str) -> Pool {
        Pool {
            name: name.to_string(),
            gitlab_runner_id: runner_id,
            auth_token: "token".into(),
            tags: tags.to_string(),
            executor: "docker".into(),
            min_warm: 1,
            max_managers: 1,
            concurrent: 1,
            request_concurrency: 1,
            paused: false,
            trust_tier: "trusted".into(),
            cluster_alias: None,
            backend_type: "docker".into(),
        }
    }

    fn runner(id: i64, description: &str) -> RunnerInfo {
        RunnerInfo {
            id,
            description: Some(description.to_string()),
            paused: Some(false),
            tag_list: Vec::new(),
            run_untagged: true,
        }
    }

    fn manager(container_id: &str, state: &str) -> Manager {
        Manager {
            id: format!("manager-{container_id}"),
            pool_name: crate::config::STANDARD_POOL_NAME.to_string(),
            docker_container_id: container_id.to_string(),
            system_id: None,
            state: state.to_string(),
            config_dir: "/tmp/jeryu-runner".to_string(),
            started_at: None,
            last_contact_at: None,
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
    fn stale_standard_runners_are_reported_not_pruned_by_default() {
        let pools = vec![pool(crate::config::STANDARD_POOL_NAME, 1, "")];
        let runners = vec![
            runner(1, "jeryu-default"),
            runner(2, "jeryu-default"),
            runner(3, "unrelated"),
        ];

        let stale = stale_standard_runners(&pools, &runners);

        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, 2);
    }

    #[test]
    fn standard_pool_stale_db_tags_are_repairable_errors() {
        let pools = vec![pool(crate::config::STANDARD_POOL_NAME, 1, "ci,old")];
        let runners = vec![runner(1, "jeryu-default")];
        let mut issues = Vec::new();

        inspect_standard_pool_policy(&pools, &runners, &mut issues);

        assert!(issues.iter().any(|issue| {
            issue.code == "stale_db_tags" && issue.severity == "error" && issue.repairable
        }));
    }

    #[test]
    fn orphaned_local_runner_containers_ignore_active_and_draining_managers() {
        let managers = vec![
            manager("active-container-full-id", "online"),
            manager("draining-container-full-id", "draining"),
            manager("stopped-container-full-id", "stopped"),
        ];
        let containers = vec![
            container("active-container-full-id", "/jeryu-runner-active"),
            container("draining-container-full-id", "/jeryu-runner-draining"),
            container("stopped-container-full-id", "/jeryu-runner-stopped"),
            container("untracked-container-full-id", "/jeryu-runner-untracked"),
            container("unrelated-container-full-id", "/other-service"),
        ];

        let orphans = orphaned_local_runner_containers(&containers, &managers);

        let orphan_ids = orphans
            .iter()
            .filter_map(|container| container.id.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(
            orphan_ids,
            vec!["stopped-container-full-id", "untracked-container-full-id"]
        );
    }
}
