//! Named hard-stop / risk-escalation condition registry.
//!
//! Policy YAML (`approvals.yml::hard_stops`, `risk.yml` matchers) reference
//! vetted *names* defined here. Unknown names fail closed. Adding a new
//! condition is a code change reviewed at R4 — there is no runtime string-eval,
//! no expression parser (Decision: YAML-only, no DSL).
//!
//! Some conditions are deterministic (pack-local); others are
//! `cond_externally_supplied` — the judge/orchestrator pre-computes them and
//! injects them by name. Unknown name → `unknown_condition:<name>` (fail-closed).
//!
//! The named conditions themselves live in responsibility-scoped submodules
//! ([`security`], [`review`], [`supply_chain`], [`anti_vibe`], [`external`]);
//! this module owns the registry plumbing that wires them up by name.

mod anti_vibe;
mod ci;
mod external;
mod paths;
mod review;
mod security;
mod supply_chain;

use crate::types::{AgentApprovalReceipt, EvidencePack};
use serde::{Deserialize, Serialize};

use anti_vibe::{
    cond_changes_agent_prompts_or_judge_policy, cond_changes_release_or_deploy_policy,
    cond_coverage_threshold_lowered, cond_removes_or_weakens_tests, cond_snapshot_mass_replacement,
};
use ci::{
    FAILED_REQUIRED_CI_CHECK, MISSING_REQUIRED_CI_CHECK, cond_failed_required_ci_check,
    cond_missing_required_ci_check,
};

/// The required-check / CI hard-stop evaluator. Re-exported for the judge, which
/// holds both the pack and the policy's `required_ci_lanes` and merges the
/// computed hits into the hard-stop walk (veto > approval).
pub use ci::ci_hard_stops;
use external::cond_externally_supplied;
use review::{
    cond_prompt_injection_suspected, cond_reviewer_abstained_required, cond_reviewer_blocked,
};
use security::{
    cond_changes_security_scanner_config, cond_dependency_scan_failed, cond_evidence_missing,
    cond_evidence_signature_invalid, cond_sast_failed, cond_secret_scan_failed,
    cond_secret_scan_missing, cond_touches_secret_handling,
};
use supply_chain::{
    cond_introduces_new_external_code_source, cond_lockfile_diff_without_manifest_diff,
    cond_lockfile_only_change,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardStop {
    pub name: String,
    pub reason: String,
    #[serde(default)]
    pub details: serde_json::Value,
}

/// Signature of a named condition: takes the pack + receipts, returns
/// `Some(HardStop)` if it triggers, else `None`.
pub type CondFn = fn(&EvidencePack, &[AgentApprovalReceipt]) -> Option<HardStop>;

#[derive(Debug, Clone, Copy)]
pub struct NamedCondition {
    pub name: &'static str,
    pub func: CondFn,
}

pub struct ConditionRegistry {
    table: Vec<NamedCondition>,
}

impl Default for ConditionRegistry {
    fn default() -> Self {
        // Every named condition that may appear in `.jeryu/autonomy/policies/*.yml`
        // MUST be registered here. Conditions needing richer context than
        // `(EvidencePack, &[Receipt])` are registered as
        // `cond_externally_supplied`, which returns None unless the caller
        // injects the name. This keeps the registry total (no unknown-condition
        // fail-closes) without faking logic we don't have.
        let table = vec![
            // Implemented locally
            NamedCondition {
                name: "evidence_missing",
                func: cond_evidence_missing,
            },
            NamedCondition {
                name: "evidence_signature_invalid",
                func: cond_evidence_signature_invalid,
            },
            NamedCondition {
                name: "secret_scan_failed",
                func: cond_secret_scan_failed,
            },
            NamedCondition {
                name: "secret_scan_missing",
                func: cond_secret_scan_missing,
            },
            NamedCondition {
                name: "sast_failed",
                func: cond_sast_failed,
            },
            NamedCondition {
                name: "dependency_scan_failed",
                func: cond_dependency_scan_failed,
            },
            NamedCondition {
                name: "reviewer_blocked",
                func: cond_reviewer_blocked,
            },
            NamedCondition {
                name: "reviewer_abstained_required",
                func: cond_reviewer_abstained_required,
            },
            NamedCondition {
                name: "lockfile_only_change",
                func: cond_lockfile_only_change,
            },
            NamedCondition {
                name: "prompt_injection_suspected",
                func: cond_prompt_injection_suspected,
            },
            // Required-check / CI gate. Registered so policy may reference the
            // names; the actual hit (which needs the policy's
            // `required_ci_lanes`, not pack data) is computed by the judge via
            // `ci::ci_hard_stops` and merged into the hard-stop walk. Locally
            // these are no-ops, like the externally-supplied conditions.
            NamedCondition {
                name: MISSING_REQUIRED_CI_CHECK,
                func: cond_missing_required_ci_check,
            },
            NamedCondition {
                name: FAILED_REQUIRED_CI_CHECK,
                func: cond_failed_required_ci_check,
            },
            // Deterministic detectors
            NamedCondition {
                name: "coverage_threshold_lowered",
                func: cond_coverage_threshold_lowered,
            },
            NamedCondition {
                name: "snapshot_mass_replacement",
                func: cond_snapshot_mass_replacement,
            },
            // Externally supplied (judge / orchestrator injects via external_hard_stops)
            NamedCondition {
                name: "sha_drift",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "policy_sha_drift",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "missing_required_review_role",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "missing_evidence_pack",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "codeowners_not_satisfied",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "freeze_window_active",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "budget_exceeded",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "training_use_required_but_disallowed",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "judge_signature_invalid",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "changes_security_scanner_config",
                func: cond_changes_security_scanner_config,
            },
            NamedCondition {
                name: "changes_release_or_deploy_policy",
                func: cond_changes_release_or_deploy_policy,
            },
            NamedCondition {
                name: "changes_agent_prompts_or_judge_policy",
                func: cond_changes_agent_prompts_or_judge_policy,
            },
            NamedCondition {
                name: "touches_secret_handling",
                func: cond_touches_secret_handling,
            },
            NamedCondition {
                name: "destructive_database_change",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "removes_or_weakens_tests",
                func: cond_removes_or_weakens_tests,
            },
            NamedCondition {
                name: "introduces_new_external_code_source",
                func: cond_introduces_new_external_code_source,
            },
            NamedCondition {
                name: "lockfile_diff_without_manifest_diff",
                func: cond_lockfile_diff_without_manifest_diff,
            },
            NamedCondition {
                name: "dependency_count_delta_gte_5",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "all_files_have_targeted_tests",
                func: cond_externally_supplied,
            },
            // Release-artifact integrity + rollback drill. All four are
            // evaluated by the release pipeline / orchestrator and injected via
            // external_hard_stops.
            NamedCondition {
                name: "release_artifact_unsigned",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "release_sbom_missing",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "release_provenance_missing",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "rollback_drill_failed",
                func: cond_externally_supplied,
            },
        ];
        Self { table }
    }
}

impl ConditionRegistry {
    pub fn lookup(&self, name: &str) -> Option<NamedCondition> {
        self.table.iter().copied().find(|c| c.name == name)
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.table.iter().map(|c| c.name).collect()
    }

    /// Evaluate every requested named condition; returns the list of triggered
    /// hard stops in requested order. Unknown names become a
    /// `unknown_condition:<name>` hard-stop (fail-closed).
    pub fn evaluate(
        &self,
        requested: &[String],
        pack: &EvidencePack,
        receipts: &[AgentApprovalReceipt],
    ) -> Vec<HardStop> {
        let mut out = Vec::new();
        for name in requested {
            match self.lookup(name) {
                Some(c) => {
                    if let Some(h) = (c.func)(pack, receipts) {
                        out.push(h);
                    }
                }
                None => out.push(HardStop {
                    name: format!("unknown_condition:{name}"),
                    reason: "policy references a condition not in the registry; fail-closed".into(),
                    details: serde_json::Value::Null,
                }),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
