//! Action registry — single source of truth for all jeryu actions.
//! Owner: TUI action surface and capability action contract.
//! Proof: `cargo test -p jeryu -- action_registry`.
//! Invariants: action IDs are unique; mutating actions declare grants; capability JSON is generated from this registry.
//! Consumed by TUI command palette, CLI `jeryu action list`, and capability `ListAllowedActions`.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[path = "action_registry_entries.rs"]
mod entries;
pub use entries::{REGISTRY, entries_for_surface, filtered};

/// Six-tier risk classification (U06).
///
/// Migrated from the original 4-tier (`ReadOnly`, `Low`, `High`, `Production`)
/// scheme. Legacy `snake_case` names continue to deserialize through
/// `#[serde(alias = ...)]` so existing JSON fixtures keep working.
///
/// - R0: read-only (no side effects)
/// - R1: local mutation (jeryu-local state only)
/// - R2: CI mutation (triggers pipelines / requeues)
/// - R3: repo write (Git branches / merge requests)
/// - R4: release / merge to production-adjacent ref
/// - R5: destructive, secret, or production-rooted (reserved; not yet used)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskTier {
    #[serde(alias = "read_only")]
    R0,
    #[serde(alias = "low")]
    R1,
    R2,
    #[serde(alias = "high")]
    R3,
    #[serde(alias = "production")]
    R4,
    R5,
}

impl RiskTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::R0 => "R0 (read)",
            Self::R1 => "R1 (local)",
            Self::R2 => "R2 (ci)",
            Self::R3 => "R3 (repo)",
            Self::R4 => "R4 (release)",
            Self::R5 => "R5 (destructive)",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::R0 => Color::Green,
            Self::R1 => Color::LightGreen,
            Self::R2 => Color::Yellow,
            Self::R3 => Color::LightRed,
            Self::R4 => Color::Red,
            Self::R5 => Color::Magenta,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Cli,
    Tui,
    Capability,
}

impl Surface {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Tui => "tui",
            Self::Capability => "capability",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Coarse class of side effect an action can perform.
pub enum SideEffectClass {
    /// Reads state only.
    ReadOnly,
    /// Mutates local jeryu state only.
    LocalState,
    /// Writes to Git branches or merge requests.
    GitWrite,
    /// Starts CI or validation work.
    CiExecution,
    /// Attempts or requests merge.
    Merge,
    /// Touches production release state.
    Production,
}

impl SideEffectClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::LocalState => "local_state",
            Self::GitWrite => "git_write",
            Self::CiExecution => "ci_execution",
            Self::Merge => "merge",
            Self::Production => "production",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Grant required before an action may run.
pub enum GrantRequirement {
    /// No grant required.
    None,
    /// Requires a scoped agent task grant.
    AgentTask,
    /// Requires merge approval.
    MergeApproval,
    /// Requires production approval.
    ProductionApproval,
}

impl GrantRequirement {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AgentTask => "agent_task",
            Self::MergeApproval => "merge_approval",
            Self::ProductionApproval => "production_approval",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ActionEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub key_hint: Option<&'static str>,
    pub risk_tier: RiskTier,
    pub surfaces: &'static [Surface],
    pub dry_run: bool,
    pub description: &'static str,
}

impl ActionEntry {
    pub fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_ascii_lowercase();
        self.id.to_ascii_lowercase().contains(q.as_str())
            || self.label.to_ascii_lowercase().contains(q.as_str())
            || self.description.to_ascii_lowercase().contains(q.as_str())
    }

    pub fn side_effect_class(&self) -> SideEffectClass {
        // The id-keyed match is the source of truth for *what kind* of side
        // effect each action has. Keep this list consistent with the R-tier
        // assigned in the registry: any action tagged R1..=R5 must appear
        // here (tested by `mutating_actions_require_grants`).
        match self.id {
            // R1 — local jeryu-state mutation.
            "remove_record" | "pause_pool" | "bug_submit" | "bug_update" | "bug_record_attempt" => {
                SideEffectClass::LocalState
            }
            // R2 — CI execution (triggers pipelines).
            "requeue_job" | "run_tests" => SideEffectClass::CiExecution,
            // R3 — Git/repo write.
            "propose_patch" | "race_patches" => SideEffectClass::GitWrite,
            // R4 — merge / release-adjacent.
            "request_merge" => SideEffectClass::Merge,
            // Everything else (R0) is a pure read.
            _ => SideEffectClass::ReadOnly,
        }
    }

    pub fn required_grant(&self) -> GrantRequirement {
        match self.side_effect_class() {
            SideEffectClass::ReadOnly => GrantRequirement::None,
            SideEffectClass::LocalState
            | SideEffectClass::GitWrite
            | SideEffectClass::CiExecution => GrantRequirement::AgentTask,
            SideEffectClass::Merge => GrantRequirement::MergeApproval,
            SideEffectClass::Production => GrantRequirement::ProductionApproval,
        }
    }

    pub fn contract_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "label": self.label,
            "key_hint": self.key_hint,
            "risk_tier": self.risk_tier.label(),
            "side_effect_class": self.side_effect_class().label(),
            "required_grant": self.required_grant().label(),
            "dry_run": self.dry_run,
            "description": self.description,
            "surfaces": self.surfaces.iter().map(|s| s.label()).collect::<Vec<_>>(),
        })
    }
}
