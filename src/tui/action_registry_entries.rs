use super::{ActionEntry, RiskTier, Surface};

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
        id: "requeue_job",
        label: "Requeue job",
        key_hint: Some("r"),
        risk_tier: RiskTier::Low,
        surfaces: CLI_TUI,
        dry_run: false,
        description: "Requeue the selected failed or canceled job",
    },
    ActionEntry {
        id: "remove_record",
        label: "Remove local record",
        key_hint: Some("d"),
        risk_tier: RiskTier::Low,
        surfaces: TUI,
        dry_run: false,
        description: "Remove the selected job from local store (does not cancel it in GitLab)",
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
        id: "bug_submit",
        label: "Submit bug",
        key_hint: None,
        risk_tier: RiskTier::Low,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "Submit a canonical local bug report",
    },
    ActionEntry {
        id: "bug_list",
        label: "List bugs",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "List local bug tracker records",
    },
    ActionEntry {
        id: "bug_show",
        label: "Show bug",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "Show a local bug with history",
    },
    ActionEntry {
        id: "bug_ready",
        label: "Ready bugs",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "List ready unblocked local bugs",
    },
    ActionEntry {
        id: "bug_update",
        label: "Update bug",
        key_hint: None,
        risk_tier: RiskTier::Low,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "Triage or update a local bug",
    },
    ActionEntry {
        id: "bug_record_attempt",
        label: "Record bug attempt",
        key_hint: None,
        risk_tier: RiskTier::Low,
        surfaces: CAP_ONLY,
        dry_run: false,
        description: "Append attempt history to a local bug",
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
        id: "tab_bugs",
        label: "Go to Bugs tab",
        key_hint: Some("b"),
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to local bug tracker tab",
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
        id: "tab_llms",
        label: "Go to LLMs tab",
        key_hint: None,
        risk_tier: RiskTier::ReadOnly,
        surfaces: TUI,
        dry_run: false,
        description: "Switch to LLM provider and key-source policy tab",
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
    use crate::tui::action_registry::{GrantRequirement, SideEffectClass};
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
