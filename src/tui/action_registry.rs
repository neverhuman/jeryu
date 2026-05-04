//! Action registry — single source of truth for all jeryu actions.
//! Owner: TUI action surface and capability action contract.
//! Proof: `cargo test -p jeryu -- action_registry`.
//! Invariants: action IDs are unique; mutating actions declare grants; capability JSON is generated from this registry.
//! Consumed by TUI command palette, CLI `jeryu action list`, and capability `ListAllowedActions`.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            "delete_record" | "pause_pool" => SideEffectClass::LocalState,
            "propose_patch" | "race_patches" => SideEffectClass::GitWrite,
            "run_tests" => SideEffectClass::CiExecution,
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

const TUI: &[Surface] = &[Surface::Tui];
const CLI_TUI: &[Surface] = &[Surface::Cli, Surface::Tui];
const ALL: &[Surface] = &[Surface::Cli, Surface::Tui, Surface::Capability];
const CAP_CLI: &[Surface] = &[Surface::Capability, Surface::Cli];
const CAP_ONLY: &[Surface] = &[Surface::Capability];

pub static REGISTRY: &[ActionEntry] = &[
    ActionEntry {
        id: "open_logs",
        label: "Open job logs",
        key_hint: Some("Enter"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Open live log view for the selected job",
    },
    ActionEntry {
        id: "retry_job",
        label: "Retry job",
        key_hint: Some("r"),
        risk_tier: RiskTier::Low,
        surfaces: CLI_TUI,
        dry_run: false,
        description: "Retry the selected failed or canceled job",
    },
    ActionEntry {
        id: "delete_record",
        label: "Forget local record",
        key_hint: Some("d"),
        risk_tier: RiskTier::Low,
        surfaces: TUI,
        dry_run: false,
        description: "Remove the selected job from local DB (does not cancel it in GitLab)",
    },
    ActionEntry {
        id: "pause_pool",
        label: "Pause/resume pool",
        key_hint: Some("p"),
        risk_tier: RiskTier::Low,
        surfaces: CLI_TUI,
        dry_run: false,
        description: "Toggle pause on the selected runner pool",
    },
    ActionEntry {
        id: "explain_blockers",
        label: "Explain blockers",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: ALL,
        dry_run: false,
        description: "Show why the selected job, release, or merge is blocked",
    },
    ActionEntry {
        id: "fetch_capsule",
        label: "Fetch capsule",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: &[Surface::Capability],
        dry_run: false,
        description: "Fetch the latest structured failure capsule for a job",
    },
    ActionEntry {
        id: "get_system_snapshot",
        label: "System snapshot",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CAP_CLI,
        dry_run: false,
        description: "Get a full system state summary: pools, pipelines, release, cache",
    },
    ActionEntry {
        id: "get_pipeline_jobs",
        label: "Pipeline jobs",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "Fetch the downstream-expanded job list for a pipeline",
    },
    ActionEntry {
        id: "get_ci_bottlenecks",
        label: "CI bottlenecks",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "Return historical CI timing bottlenecks for a project/ref",
    },
    ActionEntry {
        id: "propose_patch",
        label: "Propose patch",
        key_hint: None,
        risk_tier: RiskTier::High,
        surfaces: ALL,
        dry_run: true,
        description: "Create a branch, apply a patch, and open an MR",
    },
    ActionEntry {
        id: "race_patches",
        label: "Race patches",
        key_hint: None,
        risk_tier: RiskTier::High,
        surfaces: CAP_CLI,
        dry_run: false,
        description: "Run multiple patch hypotheses in parallel; keep the first green",
    },
    ActionEntry {
        id: "request_merge",
        label: "Request merge",
        key_hint: None,
        risk_tier: RiskTier::Production,
        surfaces: ALL,
        dry_run: false,
        description: "Merge an MR through the risk gate (pipeline green, no selector misses, no taint)",
    },
    ActionEntry {
        id: "plan_validation",
        label: "Plan validation",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CAP_CLI,
        dry_run: false,
        description: "Validate a proposed test plan against selector miss history",
    },
    ActionEntry {
        id: "run_tests",
        label: "Run tests",
        key_hint: None,
        risk_tier: RiskTier::Low,
        surfaces: ALL,
        dry_run: false,
        description: "Trigger a targeted test pipeline in an isolated ephemeral environment",
    },
    ActionEntry {
        id: "next_action",
        label: "Show next action",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CLI_TUI,
        dry_run: false,
        description: "Print the highest-priority recommended action for the current branch",
    },
    ActionEntry {
        id: "tab_mission",
        label: "Go to Mission tab",
        key_hint: Some("1"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Mission (system health overview) tab",
    },
    ActionEntry {
        id: "tab_release",
        label: "Go to Release tab",
        key_hint: Some("2"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Release gate matrix tab",
    },
    ActionEntry {
        id: "tab_jobs",
        label: "Go to Jobs tab",
        key_hint: Some("3"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Jobs & Flow board tab",
    },
    ActionEntry {
        id: "tab_agents",
        label: "Go to Agents tab",
        key_hint: Some("4"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Agents task dashboard tab",
    },
    ActionEntry {
        id: "tab_tests",
        label: "Go to Tests tab",
        key_hint: Some("5"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Test Intelligence tab",
    },
    ActionEntry {
        id: "tab_pools",
        label: "Go to Pools tab",
        key_hint: Some("6"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Runner Pools tab",
    },
    ActionEntry {
        id: "tab_cache",
        label: "Go to Cache tab",
        key_hint: Some("7"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to SmartCache metrics tab",
    },
    ActionEntry {
        id: "tab_evidence",
        label: "Go to Evidence/Audit tab",
        key_hint: Some("8"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Evidence & Audit event ledger tab",
    },
    ActionEntry {
        id: "tab_secrets",
        label: "Go to Secrets tab",
        key_hint: Some("9"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to Vault / Secrets lifecycle tab",
    },
    ActionEntry {
        id: "toggle_audit_ledger",
        label: "Toggle audit ledger view",
        key_hint: Some("a"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "In Evidence tab: toggle between capsule list and event audit ledger",
    },
    ActionEntry {
        id: "quit",
        label: "Quit jeryu TUI",
        key_hint: Some("q"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Exit the TUI",
    },
];

/// Returns entries matching `query` (substring match on id, label, description).
pub fn filtered(query: &str) -> impl Iterator<Item = &'static ActionEntry> {
    REGISTRY.iter().filter(move |a| a.matches_query(query))
}

pub fn entries_for_surface(surface: Surface) -> impl Iterator<Item = &'static ActionEntry> {
    REGISTRY
        .iter()
        .filter(move |entry| entry.surfaces.contains(&surface))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn action_ids_are_unique() {
        let mut seen = HashSet::new();
        for entry in REGISTRY {
            assert!(seen.insert(entry.id), "duplicate action id: {}", entry.id);
        }
    }

    #[test]
    fn mutating_actions_require_grants() {
        for entry in REGISTRY {
            if entry.side_effect_class() != SideEffectClass::ReadOnly {
                assert_ne!(
                    entry.required_grant(),
                    GrantRequirement::None,
                    "{} mutates but requires no grant",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn capability_contract_contains_required_fields() {
        for entry in entries_for_surface(Surface::Capability) {
            let contract = entry.contract_json();
            assert!(contract.get("id").is_some());
            assert!(contract.get("risk_tier").is_some());
            assert!(contract.get("side_effect_class").is_some());
            assert!(contract.get("required_grant").is_some());
            assert!(contract.get("surfaces").is_some());
        }
    }
}
