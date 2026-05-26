//! Evidence lens selectors, rendering, graph derivation, and local navigation.
//!
//! Rendering stays pure over `EvidenceLensInput`; no backend I/O is allowed here.

pub mod bundle;
pub mod data;
pub mod graph;
pub mod nav;
pub mod view;

pub use bundle::{RedactedBundlePreview, redact_bundle_text};
pub use data::{
    EvidenceLensInput, EvidenceSearch, ProofHit, ReceiptHit, select_evidence_lens_input,
};
pub use graph::{
    EvidenceGraph, EvidenceGraphEdge, EvidenceGraphNode, EvidenceGraphNodeKind,
    build_entity_proof_graph,
};
pub use nav::{EvidenceNavOutcome, EvidencePane, activate_pane, move_focus};
pub use view::EvidenceLens;

#[cfg(test)]
mod tests;
