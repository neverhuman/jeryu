//! Unified entity model for the read-model contract.
//!
//! Every TUI/web-rendered object maps to exactly one [`EntityKind`]; entity IDs
//! are globally unique within a kind. Provider-neutral: no SCM-vendor names.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::risk::RiskTier;

// ── Entity Kind ─────────────────────────────────────────────────────────

/// Exhaustive taxonomy of control-plane entities.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Fleet,
    RepoFamily,
    Repo,
    Branch,
    Commit,
    PullRequest,
    CiRun,
    CiRunBridge,
    Stage,
    Job,
    RunnerPool,
    RunnerManager,
    Runner,
    Node,
    CacheObject,
    CacheRequest,
    CacheVerdict,
    CacheTaint,
    TestPlan,
    TestCase,
    SelectorMiss,
    Agent,
    AgentSession,
    AgentTask,
    AgentRace,
    Bug,
    BugAttempt,
    GitRefUpdate,
    AdmissionDecision,
    CapabilityIntent,
    CapabilityGrant,
    JankuraiRun,
    JankuraiFinding,
    AerFinding,
    SecurityFinding,
    SecretAuthority,
    ReleaseSecretSet,
    Artifact,
    Signature,
    Sbom,
    ReleaseAttempt,
    ReleaseGate,
    Evidence,
    AuditEvent,
    LlmCall,
    Source,
    Action,
    Project,
    EvidenceCapsule,
    SecretAccess,
    Grant,
    Pool,
    System,
}

impl EntityKind {
    pub const ALL: &'static [Self] = &[
        Self::Fleet,
        Self::RepoFamily,
        Self::Repo,
        Self::Branch,
        Self::Commit,
        Self::PullRequest,
        Self::CiRun,
        Self::CiRunBridge,
        Self::Stage,
        Self::Job,
        Self::RunnerPool,
        Self::RunnerManager,
        Self::Runner,
        Self::Node,
        Self::CacheObject,
        Self::CacheRequest,
        Self::CacheVerdict,
        Self::CacheTaint,
        Self::TestPlan,
        Self::TestCase,
        Self::SelectorMiss,
        Self::Agent,
        Self::AgentSession,
        Self::AgentTask,
        Self::AgentRace,
        Self::Bug,
        Self::BugAttempt,
        Self::GitRefUpdate,
        Self::AdmissionDecision,
        Self::CapabilityIntent,
        Self::CapabilityGrant,
        Self::JankuraiRun,
        Self::JankuraiFinding,
        Self::AerFinding,
        Self::SecurityFinding,
        Self::SecretAuthority,
        Self::ReleaseSecretSet,
        Self::Artifact,
        Self::Signature,
        Self::Sbom,
        Self::ReleaseAttempt,
        Self::ReleaseGate,
        Self::Evidence,
        Self::AuditEvent,
        Self::LlmCall,
        Self::Source,
        Self::Action,
        Self::Project,
        Self::EvidenceCapsule,
        Self::SecretAccess,
        Self::Grant,
        Self::Pool,
        Self::System,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fleet => "fleet",
            Self::RepoFamily => "repo_family",
            Self::Repo => "repo",
            Self::Branch => "branch",
            Self::Commit => "commit",
            Self::PullRequest => "pr",
            Self::CiRun => "ci_run",
            Self::CiRunBridge => "ci_run_bridge",
            Self::Stage => "stage",
            Self::Job => "job",
            Self::RunnerPool => "runner_pool",
            Self::RunnerManager => "runner_manager",
            Self::Runner => "runner",
            Self::Node => "node",
            Self::CacheObject => "cache_object",
            Self::CacheRequest => "cache_request",
            Self::CacheVerdict => "cache_verdict",
            Self::CacheTaint => "taint",
            Self::TestPlan => "test_plan",
            Self::TestCase => "test_case",
            Self::SelectorMiss => "selector_miss",
            Self::Agent => "agent",
            Self::AgentSession => "agent_session",
            Self::AgentTask => "agent_task",
            Self::AgentRace => "agent_race",
            Self::Bug => "bug",
            Self::BugAttempt => "bug_attempt",
            Self::GitRefUpdate => "git_ref_update",
            Self::AdmissionDecision => "admission_decision",
            Self::CapabilityIntent => "capability_intent",
            Self::CapabilityGrant => "capability_grant",
            Self::JankuraiRun => "jankurai_run",
            Self::JankuraiFinding => "jankurai_finding",
            Self::AerFinding => "aer_finding",
            Self::SecurityFinding => "security_finding",
            Self::SecretAuthority => "secret_authority",
            Self::ReleaseSecretSet => "release_secret_set",
            Self::Artifact => "artifact",
            Self::Signature => "signature",
            Self::Sbom => "sbom",
            Self::ReleaseAttempt => "release",
            Self::ReleaseGate => "gate",
            Self::Evidence => "evidence",
            Self::AuditEvent => "audit_event",
            Self::LlmCall => "llm_call",
            Self::Source => "source",
            Self::Action => "action",
            Self::Project => "project",
            Self::EvidenceCapsule => "capsule",
            Self::SecretAccess => "secret",
            Self::Grant => "grant",
            Self::Pool => "pool",
            Self::System => "system",
        }
    }

    pub fn route_segment(self) -> &'static str {
        match self {
            Self::PullRequest => "pull-requests",
            Self::EvidenceCapsule => "evidence-capsules",
            Self::ReleaseAttempt => "release-attempts",
            Self::ReleaseGate => "release-gates",
            Self::SecretAccess => "secret-access",
            Self::Pool => "pools",
            _ => self.label(),
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Fleet => "FLT",
            Self::RepoFamily => "FAM",
            Self::Repo | Self::Project => "REP",
            Self::Branch => "BR",
            Self::Commit => "SHA",
            Self::PullRequest => "PR",
            Self::CiRun | Self::CiRunBridge | Self::Stage => "CI",
            Self::Job => "JOB",
            Self::RunnerPool | Self::RunnerManager | Self::Runner | Self::Pool | Self::Node => {
                "RUN"
            }
            Self::CacheObject | Self::CacheRequest | Self::CacheVerdict | Self::CacheTaint => "CAC",
            Self::TestPlan | Self::TestCase | Self::SelectorMiss => "TST",
            Self::Agent | Self::AgentSession | Self::AgentTask | Self::AgentRace => "AGT",
            Self::Bug | Self::BugAttempt => "BUG",
            Self::GitRefUpdate | Self::AdmissionDecision => "GIT",
            Self::CapabilityIntent | Self::CapabilityGrant | Self::Grant => "CAP",
            Self::JankuraiRun | Self::JankuraiFinding => "JNK",
            Self::AerFinding => "AER",
            Self::SecurityFinding
            | Self::SecretAuthority
            | Self::ReleaseSecretSet
            | Self::SecretAccess => "SEC",
            Self::Artifact | Self::Signature | Self::Sbom => "ART",
            Self::ReleaseAttempt | Self::ReleaseGate => "REL",
            Self::Evidence | Self::EvidenceCapsule | Self::AuditEvent => "EVD",
            Self::LlmCall => "LLM",
            Self::Source => "SRC",
            Self::Action => "ACT",
            Self::System => "SYS",
        }
    }
}

// ── Entity Reference ────────────────────────────────────────────────────

/// Lightweight pointer to any entity in the control plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
}

impl EntityRef {
    pub fn new(kind: EntityKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    /// Human-friendly display: `job:14445`, `agent:wrath-17`, etc.
    pub fn display(&self) -> String {
        format!("{}:{}", self.kind.label(), self.id)
    }
}

impl std::fmt::Display for EntityRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind.label(), self.id)
    }
}

// ── Severity ────────────────────────────────────────────────────────────

/// Event/attention severity, ordered from most to least urgent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Blocks release or production; requires immediate action.
    Critical,
    /// Blocks merge or agent progress; should be addressed soon.
    Error,
    /// Degraded state; may self-resolve.
    Warning,
    /// Informational; no action needed.
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "P0",
            Self::Error => "P1",
            Self::Warning => "P2",
            Self::Info => "info",
        }
    }
}

// ── Health Level ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Healthy,
    Warning,
    Degraded,
    Critical,
    Unknown,
}

impl HealthLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "HEALTHY",
            Self::Warning => "WARNING",
            Self::Degraded => "DEGRADED",
            Self::Critical => "CRITICAL",
            Self::Unknown => "UNKNOWN",
        }
    }
}

// ── Shared support types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub summary: String,
    pub severity: Severity,
    pub entity: Option<EntityRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockerSummary {
    pub kind: String,
    pub severity: Severity,
    pub summary: String,
    pub entity: Option<EntityRef>,
    pub recommended_action: Option<ActionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceRef {
    pub kind: String,
    pub id: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionRef {
    pub action_id: String,
    pub label: String,
    pub risk: Option<RiskTier>,
}

impl ActionRef {
    pub fn new(action_id: impl Into<String>, label: impl Into<String>, risk: RiskTier) -> Self {
        Self {
            action_id: action_id.into(),
            label: label.into(),
            risk: Some(risk),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bug {
    pub id: String,
    pub title: String,
    pub target_project: String,
    pub source_project: String,
    pub status: String,
    pub severity: String,
    pub priority: String,
    pub difficulty: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BugAttempt {
    pub id: i64,
    pub bug_id: String,
    pub status: String,
    pub agent: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub alias: String,
    pub repo_slug: String,
    pub provider_kind: String,
    pub default_branch: String,
}

/// Per-source freshness watermarks so the TUI/web can show freshness indicators
/// per panel. Provider-neutral: `scm_ms` is the source-control watermark.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DataFreshness {
    pub scm_ms: Option<u64>,
    pub state_store_ms: Option<u64>,
    pub sandbox_ms: Option<u64>,
    pub cache_ms: Option<u64>,
    pub vault_ms: Option<u64>,
    pub overall_stale: bool,
}

// ── Entity Detail (Inspector contract) ──────────────────────────────────

/// Full detail payload for the right-side inspector.
/// Every entity kind must populate this structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDetail {
    pub entity: EntityRef,
    pub state: String,
    pub summary: String,
    pub timeline: Vec<TimelineEvent>,
    pub blockers: Vec<BlockerSummary>,
    pub evidence: Vec<EvidenceRef>,
    pub related: Vec<EntityRef>,
    pub available_actions: Vec<ActionRef>,
    pub risk: Option<RiskTier>,
    pub last_updated: Option<DateTime<Utc>>,
    pub expires_after_ms: Option<u64>,
}

impl Default for EntityDetail {
    fn default() -> Self {
        Self {
            entity: EntityRef::new(EntityKind::System, "unknown"),
            state: "unknown".into(),
            summary: String::new(),
            timeline: Vec::new(),
            blockers: Vec::new(),
            evidence: Vec::new(),
            related: Vec::new(),
            available_actions: Vec::new(),
            risk: None,
            last_updated: None,
            expires_after_ms: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_ref_display_uses_label() {
        let r = EntityRef::new(EntityKind::Job, "14445");
        assert_eq!(r.display(), "job:14445");
        assert_eq!(format!("{r}"), "job:14445");
    }

    #[test]
    fn pull_request_kind_is_provider_neutral() {
        assert_eq!(EntityKind::PullRequest.label(), "pr");
        assert_eq!(EntityKind::PullRequest.badge(), "PR");
        assert_eq!(EntityKind::PullRequest.route_segment(), "pull-requests");
    }

    #[test]
    fn all_entity_kinds_have_distinct_serde_tags() {
        // Every kind must round-trip through serde and ALL must be exhaustive.
        for kind in EntityKind::ALL {
            let json = serde_json::to_string(kind).unwrap();
            let back: EntityKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*kind, back);
        }
    }

    #[test]
    fn severity_orders_critical_first() {
        assert!(Severity::Critical < Severity::Error);
        assert!(Severity::Error < Severity::Warning);
        assert!(Severity::Warning < Severity::Info);
    }
}
