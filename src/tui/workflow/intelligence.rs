//! Owner: Interactive TUI subsystem — Delivery intelligence layer
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::intelligence`
//! Invariants: All compute functions are pure; never mutate the snapshot.
//!
//! Surfaces high-level questions a CI Production Manager actually asks:
//!   * Where is the first blocker?
//!   * What is the longest remaining (critical) path?
//!   * If this node fails, how many downstream nodes are impacted?
//!   * Which running nodes are stalled (running longer than expected)?
//!   * Can we ship this PR right now?

use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};

use super::model::*;

/// Heuristic factor: a running node is "stalled" once it has been running
/// longer than `eta_secs * STALL_FACTOR` (with a sensible floor of 90s).
const STALL_FACTOR: f64 = 1.5;
const STALL_FLOOR_SECS: u64 = 90;

/// Find the first blocker — the earliest failing/blocked node in
/// canonical/phase order.
pub fn compute_first_blocker(snap: &WorkflowSnapshot) -> Option<&WorkflowNode> {
    for phase in &snap.phases {
        for nid in &phase.node_ids {
            if let Some(node) = snap.node(nid)
                && matches!(node.status, WorkflowStatus::Error | WorkflowStatus::Blocked)
            {
                return Some(node);
            }
        }
    }
    None
}

/// Compute the critical (longest-ETA) path through remaining work.
/// Returns node ids in dependency order (root first, sink last). Cached and
/// terminal-passed nodes contribute zero to path weight.
pub fn compute_critical_path(snap: &WorkflowSnapshot) -> Vec<String> {
    // For each node, dist[id] = max weight of any path that ENDS at id.
    let mut dist: HashMap<&str, u64> = HashMap::new();
    let mut prev: HashMap<&str, Option<&str>> = HashMap::new();

    // Topological order from phases (already sorted).
    let topo: Vec<&str> = snap
        .phases
        .iter()
        .flat_map(|p| p.node_ids.iter().map(String::as_str))
        .collect();

    for nid in &topo {
        let node = match snap.node(nid) {
            Some(n) => n,
            None => continue,
        };
        let w = node_weight(node);

        let mut best: u64 = w;
        let mut best_parent: Option<&str> = None;
        for dep in &node.deps {
            if let Some(&d) = dist.get(dep.as_str()) {
                let cand = d.saturating_add(w);
                if cand > best {
                    best = cand;
                    best_parent = Some(dep.as_str());
                }
            }
        }
        dist.insert(nid, best);
        prev.insert(nid, best_parent);
    }

    // Tail = node with largest dist that is not yet terminal-pass.
    let tail = dist
        .iter()
        .filter(|(nid, _)| {
            snap.node(nid)
                .map(|n| {
                    !matches!(
                        n.status,
                        WorkflowStatus::Ran | WorkflowStatus::Cached | WorkflowStatus::Skipped
                    )
                })
                .unwrap_or(false)
        })
        .max_by_key(|(_, d)| *d)
        .map(|(nid, _)| *nid);

    let Some(tail) = tail else {
        return Vec::new();
    };

    // Walk prev pointers back to root.
    let mut chain: Vec<String> = vec![tail.to_string()];
    let mut cursor: Option<&str> = prev.get(tail).copied().flatten();
    while let Some(p) = cursor {
        chain.push(p.to_string());
        cursor = prev.get(p).copied().flatten();
    }
    chain.reverse();
    chain
}

/// Weight a node contributes to a critical-path search. Done/cached nodes
/// don't add weight; everything else uses ETA, then duration, then a small
/// default.
fn node_weight(node: &WorkflowNode) -> u64 {
    if matches!(
        node.status,
        WorkflowStatus::Ran | WorkflowStatus::Cached | WorkflowStatus::Skipped
    ) {
        return 0;
    }
    if let Some(eta) = node.eta_secs {
        return eta.max(1);
    }
    if let Some(dur) = node.duration_secs {
        return dur.round().max(1.0) as u64;
    }
    30 // default 30s for unknown work
}

/// How many nodes transitively depend on `node_id` (i.e. how many would be
/// blocked if `node_id` failed). The count excludes `node_id` itself.
pub fn compute_downstream_impact(snap: &WorkflowSnapshot, node_id: &str) -> usize {
    let mut visited: HashSet<&str> = HashSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    queue.push_back(node_id);
    visited.insert(node_id);

    // Precompute reverse-adjacency: parent_id → vec of child_ids.
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in &snap.nodes {
        for dep in &n.deps {
            children
                .entry(dep.as_str())
                .or_default()
                .push(n.id.as_str());
        }
    }

    while let Some(cur) = queue.pop_front() {
        if let Some(kids) = children.get(cur) {
            for k in kids {
                if visited.insert(*k) {
                    queue.push_back(*k);
                }
            }
        }
    }
    visited.len().saturating_sub(1)
}

/// Detect running nodes that have been running longer than their ETA*1.5
/// (with a 90s floor).
pub fn detect_stalls(snap: &WorkflowSnapshot, now: DateTime<Utc>) -> Vec<String> {
    let mut stalled = Vec::new();
    for n in &snap.nodes {
        if !matches!(n.status, WorkflowStatus::Running) {
            continue;
        }
        let Some(started) = n.started_at else {
            continue;
        };
        let elapsed = (now - started).num_seconds().max(0) as u64;
        let budget = n
            .eta_secs
            .map(|e| ((e as f64) * STALL_FACTOR) as u64)
            .unwrap_or(STALL_FLOOR_SECS)
            .max(STALL_FLOOR_SECS);
        if elapsed > budget {
            stalled.push(n.id.clone());
        }
    }
    stalled
}

/// Ship readiness % — share of canonical-pipeline phases (in the selected PR's
/// snapshot) that are fully terminal-passed (Ran/Cached/Skipped).
pub fn compute_ship_readiness(snap: &WorkflowSnapshot) -> f32 {
    let mut total: u32 = 0;
    let mut passed: u32 = 0;
    for phase in CanonicalPhase::ALL {
        let nodes_in_phase: Vec<&WorkflowNode> = snap
            .nodes
            .iter()
            .filter(|n| n.tags.iter().any(|t| t == phase.slug()))
            .collect();
        if nodes_in_phase.is_empty() {
            continue;
        }
        total += 1;
        let all_pass = nodes_in_phase.iter().all(|n| {
            matches!(
                n.status,
                WorkflowStatus::Ran | WorkflowStatus::Cached | WorkflowStatus::Skipped
            )
        });
        if all_pass {
            passed += 1;
        }
    }
    if total == 0 {
        0.0
    } else {
        (passed as f32 / total as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::super::delivery::build_demo_delivery;
    use super::*;

    #[test]
    fn first_blocker_in_demo_is_build_web() {
        let snap = build_demo_delivery();
        // PR 1842 has the build-web Error.
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1842)
            .unwrap();
        let blocker = compute_first_blocker(&pr.snapshot).unwrap();
        assert!(
            blocker.id.contains("build-web") || blocker.id.contains("e2e"),
            "first blocker should be in build-web or downstream e2e; got {}",
            blocker.id
        );
    }

    #[test]
    fn first_blocker_is_none_when_clean() {
        let snap = build_demo_delivery();
        // PR 1835 is healthy through canary.
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1835)
            .unwrap();
        assert!(compute_first_blocker(&pr.snapshot).is_none());
    }

    #[test]
    fn critical_path_is_non_empty_for_active_pr() {
        let snap = build_demo_delivery();
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1841)
            .unwrap();
        let path = compute_critical_path(&pr.snapshot);
        assert!(!path.is_empty());
    }

    #[test]
    fn downstream_impact_of_blocker_is_positive() {
        let snap = build_demo_delivery();
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1842)
            .unwrap();
        let blocker = compute_first_blocker(&pr.snapshot).unwrap();
        let impact = compute_downstream_impact(&pr.snapshot, &blocker.id);
        assert!(impact > 0, "build-web failure should block downstream work");
    }

    #[test]
    fn ship_readiness_for_blocked_pr_is_low() {
        let snap = build_demo_delivery();
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1842)
            .unwrap();
        let r = compute_ship_readiness(&pr.snapshot);
        assert!(r < 50.0, "blocked PR should not be near ready; got {r}");
    }

    #[test]
    fn ship_readiness_for_canary_pr_is_higher() {
        let snap = build_demo_delivery();
        let blocked = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1842)
            .unwrap();
        let canary = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1835)
            .unwrap();
        let r_blocked = compute_ship_readiness(&blocked.snapshot);
        let r_canary = compute_ship_readiness(&canary.snapshot);
        assert!(
            r_canary > r_blocked,
            "canary PR should be further along: canary={r_canary}, blocked={r_blocked}"
        );
    }

    #[test]
    fn stall_detection_requires_started_at_and_overshoot() {
        let mut snap = WorkflowSnapshot::empty();
        let now = Utc::now();
        snap.nodes.push(WorkflowNode {
            id: "running".into(),
            label: "running".into(),
            status: WorkflowStatus::Running,
            started_at: Some(now - chrono::Duration::seconds(200)),
            eta_secs: Some(30), // budget = 30 * 1.5 = 45, but floor is 90 → stalled at >90
            ..Default::default()
        });
        snap.nodes.push(WorkflowNode {
            id: "fresh".into(),
            label: "fresh".into(),
            status: WorkflowStatus::Running,
            started_at: Some(now - chrono::Duration::seconds(10)),
            eta_secs: Some(120),
            ..Default::default()
        });
        let stalls = detect_stalls(&snap, now);
        assert_eq!(stalls, vec!["running".to_string()]);
    }

    #[test]
    fn critical_path_for_clean_pr_is_empty() {
        // When every node is Ran/Cached/Skipped, no critical path remains.
        let mut snap = WorkflowSnapshot::empty();
        snap.nodes.push(WorkflowNode {
            id: "x".into(),
            status: WorkflowStatus::Ran,
            tags: vec![CanonicalPhase::PreMergeCI.slug().into()],
            ..Default::default()
        });
        let p = compute_critical_path(&snap);
        assert!(p.is_empty());
    }
}
