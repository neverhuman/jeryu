//! Owner: TUI Control-Plane API — durable event stream
//! Proof: `cargo nextest run -p jeryu -- api::events`
//! Invariants: Events have monotonic sequence numbers; freshness windows are always set;
//!             every event references at least one entity.

use super::entity::{ActionRef, EntityRef, Severity};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[path = "events_store.rs"]
mod store;
pub use store::EventStore;

// ── TUI Event ───────────────────────────────────────────────────────────

/// A single fact emitted by the control plane. The TUI renders from
/// a stream of these events instead of periodically re-scraping state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiEvent {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub kind: TuiEventKind,
    pub severity: Severity,
    /// Primary entity this event concerns.
    pub entity: EntityRef,
    /// Parent entity (e.g. pipeline for a job event).
    pub parent: Option<EntityRef>,
    /// One-line human-readable summary.
    pub summary: String,
    /// Correlation ID for tracing across subsystems.
    pub correlation_id: Option<String>,
    /// Evidence capsules / proof packets linked to this event.
    pub evidence_refs: Vec<String>,
    /// Suggested next actions in response to this event.
    pub next_actions: Vec<ActionRef>,
    /// Milliseconds after which this event's data may be considered aged.
    pub stale_after_ms: u64,
}

// ── Event Kinds ─────────────────────────────────────────────────────────

/// Exhaustive taxonomy of control-plane events.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TuiEventKind {
    // System
    SystemHealthUpdated,

    // Pipeline lifecycle
    PipelineCreated,
    PipelineUpdated,
    PipelineCompleted,
    PipelineSuperseded,

    // Job lifecycle
    JobCreated,
    JobStarted,
    JobUpdated,
    JobCompleted,
    JobFailed,
    JobRetried,

    // Logs
    JobLogChunk,
    JobLogAnnotation,
    JobFailureCapsuleCreated,

    // Test intelligence
    TestPlanCreated,
    TestSelectorMissCreated,
    TestVtiAccelerated,
    TestVtiSkipped,

    // Agent lifecycle
    AgentSessionCreated,
    AgentIntentStarted,
    AgentIntentFinished,
    AgentPatchProposed,
    AgentRaceCreated,
    AgentRaceWinnerSelected,

    // Grants & admission
    AgentGrantCreated,
    AgentGrantExpired,
    AgentGrantDenied,
    AdmissionDecisionCreated,

    // Cache
    CacheTaintCreated,
    CacheTaintCleared,
    CacheGcPlanCreated,

    // Release
    ReleaseGateUpdated,
    ReleaseCandidateCreated,
    ReleasePromoted,
    ReleaseRolledBack,

    // Security
    SecretAuditCreated,
    SecretAccessDenied,
    PolicyViolation,

    // Runner / fleet lifecycle (emitted on STATE TRANSITIONS only, never
    // every poll — keeps the stream low-noise; see Phase N in the master plan).
    RunnerNodeUnreachable,
    RunnerNodeBackOnline,
    FleetUnderfilled,
    FleetDrift,
    RunnerDiskCritical,
    RunnerOrphanedDetected,
    HungRunnerDetected,

    // Action lifecycle
    ActionPreviewed,
    ActionExecuted,
    ActionFailed,

    // Meta
    NextActionUpdated,
    SnapshotRefreshed,
}

impl TuiEventKind {
    /// Human-readable dot-separated label for display.
    pub fn label(self) -> &'static str {
        match self {
            Self::SystemHealthUpdated => "system.health.updated",
            Self::PipelineCreated => "pipeline.created",
            Self::PipelineUpdated => "pipeline.updated",
            Self::PipelineCompleted => "pipeline.completed",
            Self::PipelineSuperseded => "pipeline.superseded",
            Self::JobCreated => "job.created",
            Self::JobStarted => "job.started",
            Self::JobUpdated => "job.updated",
            Self::JobCompleted => "job.completed",
            Self::JobFailed => "job.failed",
            Self::JobRetried => "job.retried",
            Self::JobLogChunk => "job.log.chunk",
            Self::JobLogAnnotation => "job.log.annotation",
            Self::JobFailureCapsuleCreated => "job.capsule.created",
            Self::TestPlanCreated => "test.plan.created",
            Self::TestSelectorMissCreated => "test.selector_miss.created",
            Self::TestVtiAccelerated => "test.vti.accelerated",
            Self::TestVtiSkipped => "test.vti.skipped",
            Self::AgentSessionCreated => "agent.session.created",
            Self::AgentIntentStarted => "agent.intent.started",
            Self::AgentIntentFinished => "agent.intent.finished",
            Self::AgentPatchProposed => "agent.patch.proposed",
            Self::AgentRaceCreated => "agent.race.created",
            Self::AgentRaceWinnerSelected => "agent.race.winner",
            Self::AgentGrantCreated => "agent.grant.created",
            Self::AgentGrantExpired => "agent.grant.expired",
            Self::AgentGrantDenied => "agent.grant.denied",
            Self::AdmissionDecisionCreated => "admission.decision.created",
            Self::CacheTaintCreated => "cache.taint.created",
            Self::CacheTaintCleared => "cache.taint.cleared",
            Self::CacheGcPlanCreated => "cache.gc.plan.created",
            Self::ReleaseGateUpdated => "release.gate.updated",
            Self::ReleaseCandidateCreated => "release.candidate.created",
            Self::ReleasePromoted => "release.promoted",
            Self::ReleaseRolledBack => "release.rolled_back",
            Self::SecretAuditCreated => "secret.audit.created",
            Self::SecretAccessDenied => "secret.access.denied",
            Self::PolicyViolation => "policy.violation",
            Self::RunnerNodeUnreachable => "runner.node.unreachable",
            Self::RunnerNodeBackOnline => "runner.node.back_online",
            Self::FleetUnderfilled => "runner.fleet.underfilled",
            Self::FleetDrift => "runner.fleet.drift",
            Self::RunnerDiskCritical => "runner.disk.critical",
            Self::RunnerOrphanedDetected => "runner.orphaned.detected",
            Self::HungRunnerDetected => "runner.hung.detected",
            Self::ActionPreviewed => "action.previewed",
            Self::ActionExecuted => "action.executed",
            Self::ActionFailed => "action.failed",
            Self::NextActionUpdated => "next_action.updated",
            Self::SnapshotRefreshed => "snapshot.refreshed",
        }
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
