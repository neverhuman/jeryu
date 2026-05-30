//! `jeryu-readmodel` — the provider-neutral TUI/web read-model CONTRACT.
//!
//! This crate holds the immutable typed snapshot the TUI projects from (the
//! lenses are pure projections of [`TuiReadModel`]) plus the per-lens dashboard
//! contracts. It is foundational for both `jeryu-tui` and the web edge.
//!
//! Design rules honored here:
//! - **Provider-neutral data only.** No SCM-vendor names anywhere on the wire.
//!   The source-control component is [`read_model::SystemHealth::scm`] and the
//!   freshness source is [`freshness::SourceKind::Scm`] (D1 fix).
//! - **Pure contract.** No dependency on the engine crates; a thin trait
//!   [`seam`] (`ReadModelSource` / `ToolBackend` / `BugStore`) stands in for the
//!   future `jeryu-*` core that *produces* these types.
//! - Every type derives `serde::{Serialize, Deserialize}` for the HTTP/SSE
//!   inspection plane and round-trips losslessly.

pub mod dashboards;
pub mod entity;
pub mod fixture;
pub mod freshness;
pub mod health;
pub mod queue;
pub mod read_model;
pub mod repos;
pub mod risk;
pub mod seam;

// ── Curated re-exports (the contract surface other crates consume) ──────

pub use entity::{
    ActionRef, BlockerSummary, Bug, BugAttempt, DataFreshness, EntityDetail, EntityKind, EntityRef,
    EvidenceRef, HealthLevel, Project, Severity, TimelineEvent,
};
pub use fixture::{TuiReadModelBuilder, sample_read_model};
pub use freshness::{FreshnessState, SourceFreshness, SourceKind};
pub use health::{ComponentHealth, RunnerHealth};
pub use queue::{QueueJobSummary, QueuePoolSnapshot, QueueSnapshot};
pub use read_model::{
    ActionSafety, AttentionItem, MissionSnapshot, NextActionRecommendation, SCHEMA_VERSION,
    SystemHealth, TuiReadModel,
};
pub use repos::{RepoFamilySummary, RepoSummary, ReposSnapshot};
pub use risk::RiskTier;
pub use seam::{BugStore, ReadModelSource, SeamError, SeamResult, ToolBackend};

pub use dashboards::agents::{AgentItem, AgentStatus, AgentsSnapshot, AgentsSummary};
pub use dashboards::approvals::{ApprovalItem, ApprovalsSnapshot, ApprovalsSummary, CheckStatus};
pub use dashboards::evidence::{EvidenceItem, EvidenceSnapshot, EvidenceSummary, GateDecision};
pub use dashboards::release::{
    PromotionStage, ReleaseGate, ReleaseItem, ReleaseSnapshot, ReleaseSummary, SbomStatus,
};
pub use dashboards::runners::{RunnersDashboard, RunnersItem, RunnersSummary};
pub use dashboards::source_doctor::{SourceDoctorDashboard, SourceDoctorItem, SourceDoctorSummary};
pub use dashboards::workflow::{DeliveryPosture, WorkflowItem, WorkflowSnapshot, WorkflowSummary};
