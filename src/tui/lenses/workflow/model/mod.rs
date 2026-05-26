//! Split workflow model facade.
//!
//! These modules provide stable reset-era import paths while the oversized
//! legacy `tui::workflow::model` module is migrated incrementally.

pub mod node_kind;
pub mod pr_view;
pub mod snapshot;
pub mod status;

pub use node_kind::{AgentStage, Environment, WorkflowNodeKind};
pub use pr_view::{CanonicalPhase, DeliverySnapshot, FleetSummary, PrStatus, PullRequestView};
pub use snapshot::{
    AgentCallDetail, AgentFindingBrief, WorkflowBackendRef, WorkflowEdge, WorkflowEdgeKind,
    WorkflowNode, WorkflowPhase, WorkflowSnapshot, WorkflowSource, WorkflowSummary,
};
pub use status::WorkflowStatus;
