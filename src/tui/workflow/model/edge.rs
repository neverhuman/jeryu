//! Owner: Interactive TUI subsystem — workflow DAG edges (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::`

use serde::{Deserialize, Serialize};

/// A dependency edge in the workflow DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    pub kind: WorkflowEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEdgeKind {
    Dependency,
    StageOrder,
    VtiSkip,
}
