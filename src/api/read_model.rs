//! Owner: TUI Control-Plane API — read model (TUI gateway snapshot)
//! Proof: `cargo nextest run -p jeryu -- api::read_model`
//! Invariants: TUI renders from `TuiReadModel`, never from raw DB/Docker/GitLab state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::entity::{ActionRef, BlockerSummary, DataFreshness, EntityRef, HealthLevel, Severity};

/// Schema version for forward-compatibility checks.
pub const SCHEMA_VERSION: &str = "tui.v1.0";

// ── TUI Read Model ─────────────────────────────────────────────────────

/// The single typed snapshot that the TUI consumes for its first paint
/// and subsequent delta updates. Replaces ad-hoc assembly from scattered
/// DB/Docker/GitLab calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiReadModel {
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub event_cursor: u64,
    pub freshness: DataFreshness,
    pub mission: MissionSnapshot,
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
            attention: Vec::new(),
            next_action: None,
            system: SystemHealth::default(),
        }
    }
}

// ── Mission Snapshot ────────────────────────────────────────────────────

/// Top-level operational truth. Powers the Mission Control tab
/// and the header posture bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSnapshot {
    pub overall: HealthLevel,
    /// Is it safe for agents to create branches and write code?
    pub safe_to_code: bool,
    /// Are all merge gates satisfied for any pending MR?
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

/// A single entry in the left-rail attention queue.
/// Computed by the backend, ranked by severity.
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

/// The single highest-leverage action the system recommends right now.
/// Shown prominently on Mission Control and in the header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextActionRecommendation {
    pub action_ref: ActionRef,
    pub label: String,
    pub why: String,
    pub entity: Option<EntityRef>,
    pub confidence: f64,
    pub safety: ActionSafety,
    pub risk: crate::tui::action_registry::RiskTier,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub gitlab: ComponentHealth,
    pub database: ComponentHealth,
    pub docker: ComponentHealth,
    pub cache: ComponentHealth,
    pub vault: ComponentHealth,
    pub runners: RunnerHealth,
}

impl SystemHealth {
    /// Flat list of all component health checks.
    pub fn components(&self) -> Vec<&ComponentHealth> {
        vec![
            &self.gitlab,
            &self.database,
            &self.docker,
            &self.cache,
            &self.vault,
        ]
    }
}

impl Default for SystemHealth {
    fn default() -> Self {
        Self {
            gitlab: ComponentHealth::unknown("gitlab"),
            database: ComponentHealth::unknown("database"),
            docker: ComponentHealth::unknown("docker"),
            cache: ComponentHealth::unknown("cache"),
            vault: ComponentHealth::unknown("vault"),
            runners: RunnerHealth::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthLevel,
    pub latency_ms: Option<u64>,
    pub detail: Option<String>,
}

impl ComponentHealth {
    pub fn unknown(name: &str) -> Self {
        Self {
            name: name.into(),
            status: HealthLevel::Degraded,
            latency_ms: None,
            detail: Some("not yet checked".into()),
        }
    }

    pub fn ok(name: &str, latency_ms: u64) -> Self {
        Self {
            name: name.into(),
            status: HealthLevel::Healthy,
            latency_ms: Some(latency_ms),
            detail: None,
        }
    }

    /// Human-readable status label for display.
    pub fn status_label(&self) -> String {
        match self.status {
            HealthLevel::Healthy => "healthy".to_string(),
            HealthLevel::Warning => "warning".to_string(),
            HealthLevel::Degraded => "degraded".to_string(),
            HealthLevel::Critical => "critical".to_string(),
            HealthLevel::Unknown => "unknown".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunnerHealth {
    pub online: u32,
    pub busy: u32,
    pub idle: u32,
    pub degraded: u32,
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
    fn component_health_ok_reports_latency() {
        let h = ComponentHealth::ok("gitlab", 12);
        assert_eq!(h.latency_ms, Some(12));
        assert!(matches!(h.status, HealthLevel::Healthy));
    }

    #[test]
    fn component_health_unknown_is_degraded() {
        let h = ComponentHealth::unknown("vault");
        assert!(matches!(h.status, HealthLevel::Degraded));
    }
}
