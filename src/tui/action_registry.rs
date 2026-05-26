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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    ReadOnly,
    Low,
    High,
    Production,
}

impl RiskTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Low => "low",
            Self::High => "high",
            Self::Production => "production",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::ReadOnly => Color::Green,
            Self::Low => Color::Yellow,
            Self::High => Color::LightRed,
            Self::Production => Color::Red,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ActionRiskTier {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
}

impl ActionRiskTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::R0 => "R0",
            Self::R1 => "R1",
            Self::R2 => "R2",
            Self::R3 => "R3",
            Self::R4 => "R4",
            Self::R5 => "R5",
        }
    }

    pub fn requires_proof_modal(self) -> bool {
        matches!(self, Self::R4 | Self::R5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationPolicy {
    None,
    Inline,
    Preview,
    NamedConfirmation,
    ProofModal,
    ProofModalWithSecondaryApproval,
}

impl ConfirmationPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Inline => "inline",
            Self::Preview => "preview",
            Self::NamedConfirmation => "named_confirmation",
            Self::ProofModal => "proof_modal",
            Self::ProofModalWithSecondaryApproval => "proof_modal_secondary_approval",
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
        match self.id {
            "remove_record" | "pause_pool" | "bug_submit" | "bug_update" | "bug_record_attempt" => {
                SideEffectClass::LocalState
            }
            "propose_patch" | "race_patches" => SideEffectClass::GitWrite,
            "requeue_job" | "run_tests" => SideEffectClass::CiExecution,
            "request_merge" => SideEffectClass::Merge,
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

    pub fn action_risk_tier(&self) -> ActionRiskTier {
        match self.side_effect_class() {
            SideEffectClass::ReadOnly => ActionRiskTier::R0,
            SideEffectClass::LocalState => ActionRiskTier::R1,
            SideEffectClass::CiExecution => ActionRiskTier::R2,
            SideEffectClass::GitWrite => ActionRiskTier::R3,
            SideEffectClass::Merge => ActionRiskTier::R4,
            SideEffectClass::Production => ActionRiskTier::R5,
        }
    }

    pub fn confirmation_policy(&self) -> ConfirmationPolicy {
        match self.action_risk_tier() {
            ActionRiskTier::R0 => ConfirmationPolicy::None,
            ActionRiskTier::R1 => ConfirmationPolicy::Inline,
            ActionRiskTier::R2 => ConfirmationPolicy::Preview,
            ActionRiskTier::R3 => ConfirmationPolicy::NamedConfirmation,
            ActionRiskTier::R4 => ConfirmationPolicy::ProofModal,
            ActionRiskTier::R5 => ConfirmationPolicy::ProofModalWithSecondaryApproval,
        }
    }

    pub fn contract_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "label": self.label,
            "key_hint": self.key_hint,
            "risk_tier": self.risk_tier.label(),
            "action_risk_tier": self.action_risk_tier().label(),
            "side_effect_class": self.side_effect_class().label(),
            "required_grant": self.required_grant().label(),
            "dry_run": self.dry_run,
            "dry_run_supported": self.dry_run,
            "confirmation_policy": self.confirmation_policy().label(),
            "description": self.description,
            "surfaces": self.surfaces.iter().map(|s| s.label()).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
#[path = "action_registry_tests.rs"]
mod tests;
