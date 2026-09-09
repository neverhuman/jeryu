#![doc = "RustJet is the Jeryu Phase 5 Rust CI acceleration kernel."]
#![forbid(unsafe_code)]

pub mod benchmarks;
pub mod changes;
pub mod classifier;
pub mod error;
pub mod explain;
pub mod features;
pub mod graph;
pub mod manifest;
pub mod nextest;
pub mod pathset;
pub mod public_api;
pub mod report;
pub mod sccache;
pub mod sharding;

pub use benchmarks::{BenchmarkExpectation, BenchmarkResult};
pub use changes::{ChangeSet, ChangedPath};
pub use classifier::{
    AffectedPackage, AffectedPlan, AffectedPlanner, ImpactReason, PlannerOptions, RunnerClass,
};
pub use error::{RustJetError, RustJetResult};
pub use explain::{ExplainFormat, ExplainOutput};
pub use features::FeatureSelection;
pub use graph::WorkspaceGraph;
pub use manifest::{PackageId, WorkspaceManifest, WorkspacePackage};
pub use nextest::{NextestCommand, NextestPlanner, count_partition_for, partition_membership};
pub use public_api::{PublicApiChange, PublicApiDetector};
pub use sccache::{SccacheDecision, SccacheMode, SccachePolicy, TrustTier};
pub use sharding::{Shard, ShardPlan};
