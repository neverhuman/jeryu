//! Owner: Interactive TUI subsystem — flow graph builder
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: Flow graphs are derived from job events and preserve lane/column classification deterministically.

use super::model::{
    BackendRef, FlowColumn, FlowColumnKind, FlowEdge, FlowGraph, FlowNode, LaneGroup, LaneKind,
};
use crate::state::JobEvent;
use crate::tui::live::{is_live_job_status, is_terminal_job_status};
use std::collections::{BTreeMap, HashMap};

pub fn classify_column(job_name: &str) -> FlowColumnKind {
    let lower = job_name.to_lowercase();
    if lower.contains("hook") || lower.contains("policy") || lower.contains("admission") {
        FlowColumnKind::Admission
    } else if lower.contains("impact") || lower.contains("plan") {
        FlowColumnKind::Impact
    } else if lower.contains("build") || lower.contains("compile") || lower.contains("image") {
        FlowColumnKind::Build
    } else if lower.contains("test")
        || lower.contains("unit")
        || lower.contains("integration")
        || lower.contains("e2e")
        || lower.contains("lint")
        || lower.contains("fmt")
    {
        FlowColumnKind::Tests
    } else if lower.contains("security")
        || lower.contains("secret")
        || lower.contains("honeypot")
        || lower.contains("guard")
    {
        FlowColumnKind::Security
    } else if lower.contains("package") || lower.contains("publish") {
        FlowColumnKind::Package
    } else if lower.contains("gate") || lower.contains("telemetry") {
        FlowColumnKind::ReleaseGates
    } else if lower.contains("canary") {
        FlowColumnKind::Canary
    } else if lower.contains("prod") || lower.contains("deploy") {
        FlowColumnKind::Production
    } else {
        FlowColumnKind::Other
    }
}

pub fn classify_lane(job_name: &str) -> LaneKind {
    let lower = job_name.to_lowercase();
    if lower.contains("unit") || lower.contains("local") || lower.contains("lib") {
        LaneKind::Unit
    } else if lower.contains("integration") || lower.contains("e2e") || lower.contains("live") {
        LaneKind::Integration
    } else if lower.contains("security") || lower.contains("secret") || lower.contains("guard") {
        LaneKind::Security
    } else if lower.contains("build") || lower.contains("compile") {
        LaneKind::Build
    } else if lower.contains("admission") || lower.contains("hook") {
        LaneKind::Admission
    } else if lower.contains("canary")
        || lower.contains("prod")
        || lower.contains("deploy")
        || lower.contains("gate")
    {
        LaneKind::ReleaseExecution
    } else {
        LaneKind::Other
    }
}

/// Stable node ID: hash of (pipeline_id, job_name) so selection survives refreshes.
fn stable_node_id(pipeline_id: i64, job_name: &str) -> i64 {
    // FNV-1a 64-bit
    let mut hash: u64 = 14695981039346656037;
    for b in job_name.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    // Mix in pipeline_id to avoid cross-pipeline collisions
    hash ^= pipeline_id as u64;
    hash = hash.wrapping_mul(1099511628211);
    (hash & 0x7FFF_FFFF_FFFF_FFFF) as i64
}

pub fn build_graph(pipeline_id: i64, jobs: Vec<JobEvent>) -> FlowGraph {
    let mut nodes = Vec::new();
    let now = chrono::Utc::now();

    for job in jobs.into_iter() {
        let name = job.job_name.as_deref().unwrap_or("unknown").to_string();
        let column = classify_column(&name);
        let lane = classify_lane(&name);

        let active = is_live_job_status(job.status.as_str());
        let mut elapsed_secs = 0;
        if let Ok(st) = chrono::DateTime::parse_from_rfc3339(&job.received_at) {
            elapsed_secs = now.signed_duration_since(st).num_seconds();
        }

        let eta = if active {
            Some(super::eta::estimate_job_eta(&name, lane, elapsed_secs))
        } else {
            None
        };

        let progress_pct = match job.status.as_str() {
            "success" | "skipped" => 100,
            "failed" | "canceled" => 50,
            "running" | "pending" | "created" | "waiting_for_resource" | "preparing" => {
                if let Some(ref e) = eta {
                    let total = e.remaining_secs + elapsed_secs;
                    if total > 0 {
                        ((elapsed_secs as f64 / total as f64) * 99.0) as u16
                    } else {
                        50
                    }
                } else {
                    ((elapsed_secs as f64 / 120.0) * 100.0).min(99.0) as u16
                }
            }
            _ if is_terminal_job_status(job.status.as_str()) => 100,
            _ => 0,
        };

        let backend = BackendRef {
            project_id: job.project_id,
            job_id: job.job_id,
            pipeline_id: job.pipeline_id,
            status: job.status.clone(),
            queued_duration: job.queued_duration,
            received_at: job.received_at.clone(),
        };

        let node = FlowNode {
            id: stable_node_id(pipeline_id, &name),
            job_id: Some(job.job_id),
            label: name.clone(),
            column,
            lane,
            status: job.status.clone(),
            progress_pct,
            eta,
            is_required: true,
            is_critical_path: false, // computed after edge build
            backend: Some(backend),
            elapsed_secs,
            // v3 defaults — populated by VTI enrichment pass:
            vti_status: None,
            cache_verdict: None,
            flake_probability: None,
            capsule_id: None,
            attempt_lineage: Vec::new(),
            agent_id: None,
        };
        nodes.push(node);
    }

    // Now organize into columns and lanes Grouping
    let mut columns = Vec::new();
    let col_kinds = vec![
        FlowColumnKind::Commit,
        FlowColumnKind::Admission,
        FlowColumnKind::Impact,
        FlowColumnKind::Pipeline,
        FlowColumnKind::Build,
        FlowColumnKind::Tests,
        FlowColumnKind::Security,
        FlowColumnKind::Package,
        FlowColumnKind::ReleaseGates,
        FlowColumnKind::Canary,
        FlowColumnKind::Production,
        FlowColumnKind::Other,
    ];

    for kind in col_kinds {
        let col_nodes: Vec<&FlowNode> = nodes.iter().filter(|n| n.column == kind).collect();
        if col_nodes.is_empty() && kind != FlowColumnKind::Pipeline {
            // Always keep Pipeline col.
            continue;
        }

        // Group by lane
        let mut lanes_map: BTreeMap<LaneKind, Vec<i64>> = BTreeMap::new();
        for node in &col_nodes {
            lanes_map.entry(node.lane).or_default().push(node.id);
        }

        let mut lane_groups = Vec::new();
        for (lane_kind, node_ids) in lanes_map {
            lane_groups.push(LaneGroup {
                lane: lane_kind,
                title: lane_kind.to_str().to_string(),
                node_ids,
            });
        }

        columns.push(FlowColumn {
            key: kind,
            title: kind.to_str().to_string(),
            status: "active".to_string(), // default until job state arrives
            eta: None,                    // pipeline-level eta goes here later
            lane_groups,
        });
    }

    // Build stage-ordering edges: jobs in column N depend on all jobs in column N-1.
    let col_order: Vec<FlowColumnKind> = columns.iter().map(|c| c.key).collect();
    let mut edges = Vec::new();
    for win in col_order.windows(2) {
        let (from_kind, to_kind) = (win[0], win[1]);
        let from_ids: Vec<i64> = nodes
            .iter()
            .filter(|n| n.column == from_kind)
            .map(|n| n.id)
            .collect();
        let to_ids: Vec<i64> = nodes
            .iter()
            .filter(|n| n.column == to_kind)
            .map(|n| n.id)
            .collect();
        for &from in &from_ids {
            for &to in &to_ids {
                edges.push(FlowEdge {
                    from,
                    to,
                    kind: crate::api::snapshot::EdgeKind::StageOrder,
                });
            }
        }
    }

    // Compute critical path via DAG longest-path (expected duration = eta remaining + elapsed).
    // We use a simple topological sort over the edge graph.
    let critical_path_ids = compute_critical_path(&nodes, &edges);
    for node in &mut nodes {
        node.is_critical_path = critical_path_ids.contains(&node.id);
    }

    FlowGraph {
        columns,
        nodes,
        edges,
    }
}

/// DAG longest-path to find the critical path.
/// Each node's weight = elapsed_secs + eta.remaining_secs (or a lane default if eta is None).
fn compute_critical_path(nodes: &[FlowNode], edges: &[FlowEdge]) -> std::collections::HashSet<i64> {
    if nodes.is_empty() {
        return std::collections::HashSet::new();
    }

    // Build adjacency: node_id -> successors
    let mut successors: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut predecessors: HashMap<i64, Vec<i64>> = HashMap::new();
    for node in nodes {
        successors.entry(node.id).or_default();
        predecessors.entry(node.id).or_default();
    }
    for edge in edges {
        successors.entry(edge.from).or_default().push(edge.to);
        predecessors.entry(edge.to).or_default().push(edge.from);
    }

    // Node weight: estimated total duration
    let weight: HashMap<i64, i64> = nodes
        .iter()
        .map(|n| {
            let w = n.elapsed_secs + n.eta.as_ref().map(|e| e.remaining_secs).unwrap_or(0);
            (n.id, w.max(1))
        })
        .collect();

    // Topological sort (Kahn's algorithm)
    let mut in_degree: HashMap<i64, usize> = nodes.iter().map(|n| (n.id, 0)).collect();
    for edge in edges {
        *in_degree.entry(edge.to).or_insert(0) += 1;
    }

    let mut queue: std::collections::VecDeque<i64> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();

    // dist[id] = max cost path ending at id; prev[id] = predecessor on that path
    let mut dist: HashMap<i64, i64> = nodes
        .iter()
        .map(|n| (n.id, *weight.get(&n.id).unwrap_or(&1)))
        .collect();
    let mut prev: HashMap<i64, Option<i64>> = nodes.iter().map(|n| (n.id, None)).collect();
    let mut topo_order = Vec::new();

    while let Some(u) = queue.pop_front() {
        topo_order.push(u);
        for &v in successors.get(&u).map(|s| s.as_slice()).unwrap_or(&[]) {
            let new_dist = dist[&u] + weight.get(&v).unwrap_or(&1);
            if new_dist > dist[&v] {
                dist.insert(v, new_dist);
                prev.insert(v, Some(u));
            }
            let deg = in_degree.entry(v).or_insert(1);
            *deg = deg.saturating_sub(1);
            if *deg == 0 {
                queue.push_back(v);
            }
        }
    }

    // Find the node with the maximum distance (end of critical path)
    let Some((end_id_ref, _)) = dist.iter().max_by_key(|(_, v)| *v) else {
        return std::collections::HashSet::new();
    };
    let end_id = *end_id_ref;

    // Trace back the critical path
    let mut critical = std::collections::HashSet::new();
    let mut cur = end_id;
    loop {
        critical.insert(cur);
        match prev.get(&cur).and_then(|p| *p) {
            Some(p) => cur = p,
            None => break,
        }
    }
    critical
}
