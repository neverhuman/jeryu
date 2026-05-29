use super::*;
use std::collections::HashSet;

const CURRENT_ACTION_IDS: &[&str] = &[
    "open_logs",
    "requeue_job",
    "remove_record",
    "pause_pool",
    "explain_blockers",
    "fetch_capsule",
    "get_system_snapshot",
    "get_pipeline_jobs",
    "get_ci_bottlenecks",
    "propose_patch",
    "race_patches",
    "request_merge",
    "plan_validation",
    "bug_submit",
    "bug_list",
    "bug_show",
    "bug_ready",
    "bug_update",
    "bug_record_attempt",
    "run_tests",
    "next_action",
    "tab_mission",
    "tab_release",
    "tab_jobs",
    "tab_agents",
    "tab_tests",
    "tab_pools",
    "tab_cache",
    "tab_evidence",
    "tab_repos",
    "tab_bugs",
    "tab_secrets",
    "tab_llms",
    "toggle_audit_ledger",
    "quit",
];

#[test]
fn all_current_action_ids_resolve() {
    let registry_ids = REGISTRY
        .iter()
        .map(|entry| entry.id)
        .collect::<HashSet<_>>();
    assert_eq!(registry_ids.len(), CURRENT_ACTION_IDS.len());
    for id in CURRENT_ACTION_IDS {
        assert!(registry_ids.contains(id), "missing action id: {id}");
    }
}

#[test]
fn action_risk_tiers_follow_side_effect_class() {
    let entry = REGISTRY
        .iter()
        .find(|entry| entry.id == "request_merge")
        .unwrap();
    assert_eq!(entry.action_risk_tier(), ActionRiskTier::R4);
    assert!(entry.action_risk_tier().requires_proof_modal());

    let entry = REGISTRY
        .iter()
        .find(|entry| entry.id == "run_tests")
        .unwrap();
    assert_eq!(entry.action_risk_tier(), ActionRiskTier::R2);
    assert_eq!(entry.confirmation_policy(), ConfirmationPolicy::Preview);
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
fn contract_json_contains_reset_action_metadata() {
    for entry in REGISTRY {
        let contract = entry.contract_json();
        assert!(contract.get("action_risk_tier").is_some());
        assert!(contract.get("confirmation_policy").is_some());
        assert!(contract.get("dry_run_supported").is_some());
    }
}
