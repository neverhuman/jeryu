//! TUI/web read-model contract.
//!
//! The TUI and the web edge render from [`TuiReadModel`], never from raw
//! DB/sandbox/SCM state. This is the single immutable typed snapshot consumed
//! for first paint and subsequent delta updates.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::dashboards::runners::RunnersDashboard;
use crate::dashboards::source_doctor::SourceDoctorDashboard;
use crate::entity::{ActionRef, BlockerSummary, DataFreshness, EntityRef, HealthLevel, Severity};
use crate::health::{ComponentHealth, RunnerHealth};
use crate::queue::QueueSnapshot;
use crate::repos::ReposSnapshot;
use crate::risk::RiskTier;

/// Schema version for forward-compatibility checks.
pub const SCHEMA_VERSION: &str = "tui.v1.0";

// ── TUI Read Model ─────────────────────────────────────────────────────

/// The single typed snapshot the TUI/web consumes for its first paint and
/// subsequent delta updates. Replaces ad-hoc assembly from scattered
/// DB/sandbox/SCM calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReadModel {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub freshness: DataFreshness,
    pub mission: MissionSnapshot,
    #[serde(default)]
    pub queue: QueueSnapshot,
    #[serde(default)]
    pub repos: ReposSnapshot,
    #[serde(default)]
    pub source_doctor: SourceDoctorDashboard,
    #[serde(default)]
    pub runners: RunnersDashboard,
    pub attention: Vec<AttentionItem>,
    pub next_action: Option<NextActionRecommendation>,
    pub system: SystemHealth,
}

impl Default for TuiReadModel {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.into(),
            generated_at: Utc::now(),
            event_cursor: 0,
            freshness: DataFreshness::default(),
            mission: MissionSnapshot::default(),
            queue: QueueSnapshot::default(),
            repos: ReposSnapshot::default(),
            source_doctor: SourceDoctorDashboard::default(),
            runners: RunnersDashboard::default(),
            attention: Vec::new(),
            next_action: None,
            system: SystemHealth::default(),
        }
    }
}

// ── Mission Snapshot ────────────────────────────────────────────────────

/// Top-level operational truth. Powers the Mission Control tab and the header
/// posture bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSnapshot {
    pub overall: HealthLevel,
    /// Is it safe for agents to create branches and write code?
    pub safe_to_code: bool,
    /// Are all merge gates satisfied for any pending pull request?
    pub safe_to_merge: bool,
    /// Is there a release candidate that can ship?
    pub safe_to_release: bool,
    /// The single most important blocker right now.
    pub top_blocker: Option<BlockerSummary>,
    pub active_agents: u32,
    pub blocked_agents: u32,
    pub running_jobs: u32,
    pub failed_jobs: u32,
    pub queued_jobs: u32,
    pub open_capsules: u32,
    pub active_grants: u32,
    pub cache_hit_ratio: f64,
    pub active_taints: u32,
    pub selector_misses_24h: u32,
    // v3 — mission cockpit fields:
    pub agents_can_code: bool,
    pub active_runners: u32,
    pub total_runners: u32,
    pub evidence_count: u32,
    pub taint_count: u32,
}

impl Default for MissionSnapshot {
    fn default() -> Self {
        Self {
            overall: HealthLevel::Healthy,
            safe_to_code: true,
            safe_to_merge: false,
            safe_to_release: false,
            top_blocker: None,
            active_agents: 0,
            blocked_agents: 0,
            running_jobs: 0,
            failed_jobs: 0,
            queued_jobs: 0,
            open_capsules: 0,
            active_grants: 0,
            cache_hit_ratio: 0.0,
            active_taints: 0,
            selector_misses_24h: 0,
            agents_can_code: true,
            active_runners: 0,
            total_runners: 0,
            evidence_count: 0,
            taint_count: 0,
        }
    }
}

// ── Attention Item ──────────────────────────────────────────────────────

/// A single entry in the left-rail attention queue. Computed by the backend,
/// ranked by severity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionItem {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub why_it_matters: String,
    pub entity: EntityRef,
    pub evidence: Vec<String>,
    pub recommended_actions: Vec<ActionRef>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

// ── Next Action Recommendation ──────────────────────────────────────────

/// The single highest-leverage action the system recommends right now. Shown
/// prominently on Mission Control and in the header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextActionRecommendation {
    pub action_ref: ActionRef,
    pub label: String,
    pub why: String,
    pub entity: Option<EntityRef>,
    pub confidence: f64,
    pub safety: ActionSafety,
    pub risk: RiskTier,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionSafety {
    /// No side effects; pure read.
    Safe,
    /// Side effects, but reversible.
    Reversible,
    /// Side effects, not reversible. Requires confirmation.
    Irreversible,
    /// Touches production. Requires explicit approval.
    ProductionImpact,
}

// ── System Health ───────────────────────────────────────────────────────

/// Infrastructure health summary for the header posture bar.
///
/// D1 fix: the provider-named source-control component is the neutral `scm`
/// field (was a vendor name in the source product).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// Source-control plane health (provider-neutral).
    pub scm: ComponentHealth,
    pub database: ComponentHealth,
    /// Sandbox/runner controller health (provider-neutral; was `docker`).
    pub sandbox: ComponentHealth,
    pub cache: ComponentHealth,
    pub vault: ComponentHealth,
    pub runners: RunnerHealth,
}

impl SystemHealth {
    /// Flat list of all component health checks (excludes the runner rollup).
    pub fn components(&self) -> Vec<&ComponentHealth> {
        vec![
            &self.scm,
            &self.database,
            &self.sandbox,
            &self.cache,
            &self.vault,
        ]
    }
}

impl Default for SystemHealth {
    fn default() -> Self {
        Self {
            scm: ComponentHealth::unknown("scm"),
            database: ComponentHealth::unknown("database"),
            sandbox: ComponentHealth::unknown("sandbox"),
            cache: ComponentHealth::unknown("cache"),
            vault: ComponentHealth::unknown("vault"),
            runners: RunnerHealth::default(),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_read_model_has_schema_version() {
        let model = TuiReadModel::default();
        assert_eq!(model.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn default_mission_is_safe_to_code() {
        let mission = MissionSnapshot::default();
        assert!(mission.safe_to_code);
        assert!(!mission.safe_to_merge);
        assert!(!mission.safe_to_release);
    }

    #[test]
    fn default_read_model_has_empty_repos_snapshot() {
        let model = TuiReadModel::default();
        assert_eq!(model.repos, ReposSnapshot::default());
    }

    #[test]
    fn system_health_components_are_provider_neutral() {
        let health = SystemHealth::default();
        let names: Vec<&str> = health
            .components()
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, ["scm", "database", "sandbox", "cache", "vault"]);
    }

    #[test]
    fn system_health_serializes_scm_field() {
        let json = serde_json::to_string(&SystemHealth::default()).unwrap();
        assert!(json.contains("\"scm\""));
        // The provider-named field must be gone from the wire contract.
        assert!(!json.contains("\"docker\""));
    }
}
