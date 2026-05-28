//! Owner: Runner Fleet / Standard Pool Topology
//! Proof: `cargo test -p jeryu -- pool_topology`
//! Invariants: desired standard CI capacity is explicit per execution node.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::state::Manager;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolTopologyTarget {
    pub node_alias: String,
    pub desired: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolTopologyPlanEntry {
    pub node_alias: String,
    pub desired: usize,
    pub active: usize,
    pub delta: isize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolTopologyPlan {
    pub pool_name: String,
    pub desired_total: usize,
    pub active_total: usize,
    pub entries: Vec<PoolTopologyPlanEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolReservedNode {
    pub node_alias: String,
    pub desired: usize,
    pub active: usize,
    pub reason: String,
}

pub fn is_standard_topology_pool(pool_name: &str) -> bool {
    pool_name == config::STANDARD_POOL_NAME
}

pub fn local_node_key() -> String {
    config::LOCAL_NODE_ALIAS.to_string()
}

pub fn manager_node_key(manager: &Manager) -> String {
    manager.node_alias.clone().unwrap_or_else(local_node_key)
}

pub fn desired_standard_pool_targets() -> Vec<PoolTopologyTarget> {
    config::STANDARD_POOL_TOPOLOGY
        .iter()
        .map(|target| PoolTopologyTarget {
            node_alias: target
                .node_alias
                .unwrap_or(config::LOCAL_NODE_ALIAS)
                .to_string(),
            desired: target.managers,
        })
        .collect()
}

pub fn standard_pool_desired_total() -> usize {
    config::STANDARD_POOL_TOPOLOGY
        .iter()
        .map(|target| target.managers)
        .sum()
}

pub fn reserved_standard_pool_nodes(managers: &[Manager]) -> Vec<PoolReservedNode> {
    let active_counts = active_manager_counts_by_node(managers);
    config::STANDARD_POOL_RESERVED_NODE_ALIASES
        .iter()
        .map(|alias| PoolReservedNode {
            node_alias: (*alias).to_string(),
            desired: 0,
            active: active_counts.get(*alias).copied().unwrap_or(0),
            reason: "reserved for active agent execution".to_string(),
        })
        .collect()
}

pub fn active_manager_counts_by_node(managers: &[Manager]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for manager in managers
        .iter()
        .filter(|manager| manager_state_counts_as_topology_active(&manager.state))
    {
        *counts.entry(manager_node_key(manager)).or_insert(0) += 1;
    }
    counts
}

pub fn plan_pool_topology(
    pool_name: &str,
    managers: &[Manager],
    targets: &[PoolTopologyTarget],
) -> PoolTopologyPlan {
    let active_counts = active_manager_counts_by_node(managers);
    let mut node_aliases = targets
        .iter()
        .map(|target| target.node_alias.clone())
        .collect::<BTreeSet<_>>();
    node_aliases.extend(active_counts.keys().cloned());

    let mut entries = Vec::new();
    for node_alias in node_aliases {
        let desired = targets
            .iter()
            .find(|target| target.node_alias == node_alias)
            .map(|target| target.desired)
            .unwrap_or(0);
        let active = active_counts.get(&node_alias).copied().unwrap_or(0);
        entries.push(PoolTopologyPlanEntry {
            node_alias,
            desired,
            active,
            delta: desired as isize - active as isize,
        });
    }

    PoolTopologyPlan {
        pool_name: pool_name.to_string(),
        desired_total: entries.iter().map(|entry| entry.desired).sum(),
        active_total: entries.iter().map(|entry| entry.active).sum(),
        entries,
    }
}

fn manager_state_counts_as_topology_active(state: &str) -> bool {
    matches!(
        state,
        "starting" | "online" | "node_starting" | "node_unreachable"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(id: &str, node_alias: Option<&str>, state: &str) -> Manager {
        Manager {
            id: id.to_string(),
            pool_name: config::STANDARD_POOL_NAME.to_string(),
            docker_container_id: format!("container-{id}"),
            system_id: None,
            state: state.to_string(),
            config_dir: format!("/tmp/{id}"),
            started_at: None,
            last_contact_at: None,
            node_alias: node_alias.map(str::to_string),
        }
    }

    #[test]
    fn standard_pool_targets_are_exact_per_node_counts() {
        assert_eq!(
            desired_standard_pool_targets(),
            vec![
                PoolTopologyTarget {
                    node_alias: config::LOCAL_NODE_ALIAS.to_string(),
                    desired: 10,
                },
                PoolTopologyTarget {
                    node_alias: "xbabe0".to_string(),
                    desired: 10,
                },
                PoolTopologyTarget {
                    node_alias: "xbabe1".to_string(),
                    desired: 10,
                },
                PoolTopologyTarget {
                    node_alias: "xbabe3".to_string(),
                    desired: 10,
                },
            ]
        );
        assert_eq!(standard_pool_desired_total(), 40);
        assert_eq!(
            crate::config::STANDARD_POOL_RESERVED_NODE_ALIASES,
            &["xbabe2"]
        );
    }

    #[test]
    fn topology_plan_preserves_local_remote_and_extra_nodes() {
        let managers = vec![
            manager("local-1", None, "online"),
            manager("remote-1", Some("xbabe0"), "node_starting"),
            manager("remote-2", Some("xbabe2"), "online"),
            manager("stopped", Some("xbabe1"), "stopped"),
        ];
        let targets = vec![
            PoolTopologyTarget {
                node_alias: config::LOCAL_NODE_ALIAS.to_string(),
                desired: 2,
            },
            PoolTopologyTarget {
                node_alias: "xbabe0".to_string(),
                desired: 1,
            },
            PoolTopologyTarget {
                node_alias: "xbabe1".to_string(),
                desired: 1,
            },
        ];

        let plan = plan_pool_topology(config::STANDARD_POOL_NAME, &managers, &targets);

        assert_eq!(plan.desired_total, 4);
        assert_eq!(plan.active_total, 3);
        assert!(plan.entries.iter().any(|entry| {
            entry.node_alias == config::LOCAL_NODE_ALIAS
                && entry.active == 1
                && entry.desired == 2
                && entry.delta == 1
        }));
        assert!(plan.entries.iter().any(|entry| {
            entry.node_alias == "xbabe2"
                && entry.active == 1
                && entry.desired == 0
                && entry.delta == -1
        }));
        assert_eq!(
            reserved_standard_pool_nodes(&managers),
            vec![PoolReservedNode {
                node_alias: "xbabe2".to_string(),
                desired: 0,
                active: 1,
                reason: "reserved for active agent execution".to_string(),
            }]
        );
    }
}
