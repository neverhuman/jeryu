//! `jeryu-codegraph` is an additive workspace code graph.
//!
//! It builds a symbol index plus crate dependency edges from the workspace
//! (reusing `jeryu-rustjet`), persists them to a self-contained SQLite database
//! (mirroring the `jeryu-core` storage pattern without touching it), supports
//! impact analysis over changed paths, and enforces an export slice with a
//! fail-closed, traversal-safe containment predicate.
//!
//! This crate touches no existing jeryu crate; it only depends on
//! `jeryu-rustjet` (path) and standard storage/serialization libraries.
#![forbid(unsafe_code)]

pub mod error;
pub mod export_gate;
pub mod graph;
pub mod oracle;
pub mod slice;
pub mod storage;
pub mod tool_build;

pub use error::{CodeGraphError, Result};
pub use export_gate::{SliceDenied, enforce_export_slice, enforce_export_slice_from_diff};
pub use graph::{CodeGraph, ImpactReport};
pub use oracle::{
    CodeContextFile, CodeGraphImpactPack, CodeGraphMcpQuery, CodeGraphProvenance, CodeGraphQuery,
    CodeGraphRepoIdentity, CodeGraphService, CodegraphImpactPack, CodegraphQuery, ExcludedFile,
    GeneratedZoneHit, GraphStats, IndexReceipt, ProofLaneImpact, SymbolImpact, default_ref_name,
    query_snapshot, query_store,
};
pub use slice::{OutOfSlice, Slice};
pub use storage::{
    CodeGraphStore, CrateDepRow, FileRow, GovernanceRow, GraphSnapshot, IndexRunRow, SCHEMA,
    SymbolRefRow, SymbolRow, default_db_path,
};
pub use tool_build::{
    ToolBuildCluster, ToolBuildIgnore, ToolBuildOccurrence, ToolBuildScanConfig,
    ToolBuildScanReport, scan_tool_build_clusters,
};
