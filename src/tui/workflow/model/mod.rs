//! Owner: Interactive TUI subsystem — workflow DAG model module root (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::`
//! Invariants: WorkflowSnapshot is read-only; built by builder, consumed by widget.
//! Layout: split from monolithic `model.rs` (945 LOC) per TUI_RESET_PLAN_FINAL.md §3.2.

pub mod edge;
pub mod node_kind;
pub mod phase;
pub mod pr_view;
pub mod snapshot;
pub mod status;

#[cfg(test)]
mod tests;

pub use edge::{WorkflowEdge, WorkflowEdgeKind};
pub use node_kind::{AgentStage, Environment, WorkflowNodeKind};
pub use phase::{CanonicalPhase, WorkflowPhase};
pub use pr_view::{DeliverySnapshot, FleetSummary, PrStatus, PullRequestView};
pub use snapshot::{
    AgentCallDetail, AgentFindingBrief, WorkflowBackendRef, WorkflowNode, WorkflowSnapshot,
    WorkflowSource, WorkflowSummary,
};
pub use status::WorkflowStatus;
