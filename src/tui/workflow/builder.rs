//! Owner: Interactive TUI subsystem — workflow DAG builder
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::builder`
//! Invariants: Builder produces a topologically-sorted WorkflowSnapshot; never mutates source state.

use std::collections::{HashMap, HashSet};

use chrono::Utc;

use super::model::*;
use crate::api::snapshot::VtiStatus;

/// Build a WorkflowSnapshot from a list of nodes (with deps already set).
/// Performs topological sort and assigns nodes to depth-based phases.
pub fn build_snapshot(
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    title: &str,
    mode: &str,
    confidence: f64,
    source: WorkflowSource,
) -> WorkflowSnapshot {
    let phases = assign_phases(&nodes);
    let summary = WorkflowSummary::from_nodes(&nodes);

    WorkflowSnapshot {
        generated_at: Utc::now(),
        title: title.to_string(),
        source,
        mode: mode.to_string(),
        confidence,
        nodes,
        edges,
        phases,
        summary,
        selected_node_id: None,
        outdated: false,
    }
}

/// Assign nodes to phases using topological depth.
/// Phase 0 = no deps, Phase 1 = depends only on Phase 0, etc.
fn assign_phases(nodes: &[WorkflowNode]) -> Vec<WorkflowPhase> {
    let ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut depth: HashMap<&str, u32> = HashMap::new();
    let mut changed = true;

    // Initialize all nodes at depth 0.
    for n in nodes {
        depth.insert(&n.id, 0);
    }

    // Iteratively compute depth = max(dep depths) + 1.
    while changed {
        changed = false;
        for n in nodes {
            let max_dep = n
                .deps
                .iter()
                .filter(|d| ids.contains(d.as_str()))
                .filter_map(|d| depth.get(d.as_str()))
                .max()
                .copied()
                .unwrap_or(0);
            let target = if n.deps.iter().any(|d| ids.contains(d.as_str())) {
                max_dep + 1
            } else {
                0
            };
            if let Some(current) = depth.get_mut(n.id.as_str())
                && target > *current
            {
                *current = target;
                changed = true;
            }
        }
    }

    // Group nodes by depth.
    let max_depth = depth.values().max().copied().unwrap_or(0);
    let mut phases = Vec::new();
    for d in 0..=max_depth {
        let node_ids: Vec<String> = nodes
            .iter()
            .filter(|n| depth.get(n.id.as_str()) == Some(&d))
            .map(|n| n.id.clone())
            .collect();
        if node_ids.is_empty() {
            continue;
        }

        let title = match d {
            0 => "Phase 0 — can run now".to_string(),
            _ => format!("Phase {} — after prior gates", d),
        };
        phases.push(WorkflowPhase {
            id: format!("phase-{}", d),
            title,
            depth: d,
            node_ids,
        });
    }

    phases
}

/// Build a demo workflow for empty state.
pub fn build_demo_snapshot() -> WorkflowSnapshot {
    let nodes = vec![
        WorkflowNode {
            id: "check".into(),
            label: "cargo check".into(),
            command: Some("cargo check -p jeryu".into()),
            kind: WorkflowNodeKind::Check,
            status: WorkflowStatus::Ran,
            required: true,
            ..Default::default()
        },
        WorkflowNode {
            id: "fmt".into(),
            label: "cargo fmt".into(),
            command: Some("cargo fmt --check".into()),
            kind: WorkflowNodeKind::Lint,
            status: WorkflowStatus::Ran,
            required: true,
            ..Default::default()
        },
        WorkflowNode {
            id: "clippy".into(),
            label: "clippy".into(),
            command: Some("cargo clippy".into()),
            kind: WorkflowNodeKind::Lint,
            status: WorkflowStatus::Running,
            progress_pct: Some(68),
            required: true,
            ..Default::default()
        },
        WorkflowNode {
            id: "vti-plan".into(),
            label: "VTI plan".into(),
            command: Some("jeryu test select".into()),
            kind: WorkflowNodeKind::VtiPlan,
            status: WorkflowStatus::Ran,
            required: true,
            vti_status: Some(VtiStatus::Selected {
                reason: "3 files changed".into(),
                confidence: 0.92,
            }),
            deps: vec!["check".into()],
            ..Default::default()
        },
        WorkflowNode {
            id: "unit-tui".into(),
            label: "unit: tui".into(),
            command: Some("cargo nextest run -- tui".into()),
            kind: WorkflowNodeKind::UnitTest,
            status: WorkflowStatus::Running,
            progress_pct: Some(45),
            required: true,
            critical_path: true,
            deps: vec!["check".into(), "vti-plan".into()],
            ..Default::default()
        },
        WorkflowNode {
            id: "unit-api".into(),
            label: "unit: api".into(),
            command: Some("cargo nextest run -- api".into()),
            kind: WorkflowNodeKind::UnitTest,
            status: WorkflowStatus::Waiting,
            required: true,
            deps: vec!["check".into(), "vti-plan".into()],
            ..Default::default()
        },
        WorkflowNode {
            id: "integration".into(),
            label: "integration tests".into(),
            command: Some("cargo nextest run --tests".into()),
            kind: WorkflowNodeKind::IntegrationTest,
            status: WorkflowStatus::Waiting,
            required: true,
            deps: vec!["unit-tui".into(), "unit-api".into()],
            ..Default::default()
        },
        WorkflowNode {
            id: "merge-gate".into(),
            label: "merge eligibility".into(),
            kind: WorkflowNodeKind::ReleaseGate,
            status: WorkflowStatus::Blocked,
            required: true,
            deps: vec!["integration".into(), "clippy".into(), "fmt".into()],
            ..Default::default()
        },
    ];

    let edges = vec![
        WorkflowEdge {
            from: "check".into(),
            to: "vti-plan".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "check".into(),
            to: "unit-tui".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "check".into(),
            to: "unit-api".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "vti-plan".into(),
            to: "unit-tui".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "vti-plan".into(),
            to: "unit-api".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "unit-tui".into(),
            to: "integration".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "unit-api".into(),
            to: "integration".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "integration".into(),
            to: "merge-gate".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "clippy".into(),
            to: "merge-gate".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
        WorkflowEdge {
            from: "fmt".into(),
            to: "merge-gate".into(),
            kind: WorkflowEdgeKind::Dependency,
        },
    ];

    build_snapshot(
        nodes,
        edges,
        "Demo workflow",
        "selected",
        0.92,
        WorkflowSource::Demo,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topological_phases_are_correct() {
        let snap = build_demo_snapshot();
        assert!(snap.phases.len() >= 3, "should have at least 3 phases");

        // Phase 0 should contain check, fmt, clippy (no deps)
        let p0_ids: HashSet<_> = snap.phases[0].node_ids.iter().cloned().collect();
        assert!(p0_ids.contains("check"));
        assert!(p0_ids.contains("fmt"));
        assert!(p0_ids.contains("clippy"));

        // merge-gate should be in the last phase
        let last = snap.phases.last().unwrap();
        assert!(last.node_ids.contains(&"merge-gate".to_string()));
    }

    #[test]
    fn phases_cover_all_nodes() {
        let snap = build_demo_snapshot();
        let total: usize = snap.phases.iter().map(|p| p.node_ids.len()).sum();
        assert_eq!(total, snap.nodes.len());
    }

    #[test]
    fn demo_summary_counts() {
        let snap = build_demo_snapshot();
        assert_eq!(snap.summary.total, 8);
        assert_eq!(snap.summary.passed, 3); // check + fmt + vti-plan
        assert_eq!(snap.summary.running, 2); // clippy + unit-tui
    }

    #[test]
    fn node_lookup_works() {
        let snap = build_demo_snapshot();
        let n = snap.node("vti-plan").unwrap();
        assert_eq!(n.kind, WorkflowNodeKind::VtiPlan);
        assert!(snap.node("nonexistent").is_none());
    }

    #[test]
    fn empty_nodes_produce_empty_phases() {
        let snap = build_snapshot(vec![], vec![], "empty", "none", 0.0, WorkflowSource::Demo);
        assert!(snap.phases.is_empty());
        assert_eq!(snap.summary.total, 0);
    }
}
