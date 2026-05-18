//! Named hard-stop / risk-escalation condition registry.
//!
//! Per decision #3 (YAML-only policy, no DSL), `policies/approvals.yml`
//! `hard_stops:` and `policies/risk.yml` `tiers[].matchers[].conditions:`
//! reference vetted *names* defined here. Unknown names fail closed to R5.
//!
//! Adding a new condition is a code change reviewed at R4 (same protection
//! Rego would give us). No runtime string-eval, no expression parser.

use crate::autonomy::types::{AgentApprovalReceipt, EvidencePack, ReviewDecision, ScanOutcome};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardStop {
    pub name: String,
    pub reason: String,
    #[serde(default)]
    pub details: serde_json::Value,
}

/// Signature of a named condition: takes the pack + receipts, returns
/// Some(HardStop) if it triggers, else None.
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
        // Every named condition that may appear in `.autonomy/policies/*.yml`
        // MUST be registered here. Some conditions require richer context
        // than `(EvidencePack, &[Receipt])` provides — those are registered as
        // `cond_externally_supplied`, which returns None unless the caller
        // injects the name into `triggered_conditions`. This keeps the
        // registry total (no unknown-condition fail-closes) without faking
        // logic we don't have.
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
            // New deterministic detectors (Wave 2.1, not yet referenced in YAML)
            NamedCondition {
                name: "coverage_threshold_lowered",
                func: cond_coverage_threshold_lowered,
            },
            NamedCondition {
                name: "snapshot_mass_replacement",
                func: cond_snapshot_mass_replacement,
            },
            // Externally supplied (judge or orchestrator injects via triggered_conditions)
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
                func: cond_externally_supplied, // requires diff semantics; defer
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
            // -----------------------------------------------------------------
            // Wave 3: release-artifact integrity (Law 6) + rollback drill
            // (Law 7). All four are evaluated by the release pipeline /
            // orchestrator (artifact signer, SBOM generator, provenance attestor,
            // rollback drill runner) and injected via JudgeInputs.external_hard_stops.
            // -----------------------------------------------------------------
            NamedCondition {
                // Law 6: build once / sign / SBOM / provenance.
                // Fires when a release artifact reaches the gate without a valid
                // signature. Externally supplied because the artifact signer
                // owns the cosign / sigstore verification path, not the pack.
                name: "release_artifact_unsigned",
                func: cond_externally_supplied,
            },
            NamedCondition {
                // Law 6: build once / sign / SBOM / provenance.
                // Fires when the release pipeline cannot locate an SBOM
                // (CycloneDX/SPDX) attached to the artifact under review.
                // Externally supplied because SBOM presence is established by
                // the build/release pipeline, not the EvidencePack.
                name: "release_sbom_missing",
                func: cond_externally_supplied,
            },
            NamedCondition {
                // Law 6: build once / sign / SBOM / provenance.
                // Fires when SLSA / in-toto provenance is missing or unverifiable
                // for the candidate artifact. Externally supplied because the
                // attestation verifier (cosign verify-attestation) is the source
                // of truth, not the pack.
                name: "release_provenance_missing",
                func: cond_externally_supplied,
            },
            NamedCondition {
                // Law 7: no rollback drill = no prod.
                // Fires when the staging rollback drill did not pass (executor
                // errored, never ran, or `RollbackDrillResult.passed == false`).
                // Externally supplied because the drill runs in the release
                // pipeline and reports back via JudgeInputs.external_hard_stops.
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

    /// Evaluate every named condition; returns the list of triggered hard stops
    /// in registry order. Unknown names in the requested list become a
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

// --- named conditions ----------------------------------------------------

fn cond_evidence_missing(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    if p.evidence_digest.is_empty() {
        return Some(HardStop {
            name: "evidence_missing".into(),
            reason: "evidence_pack has no digest".into(),
            details: serde_json::Value::Null,
        });
    }
    None
}

fn cond_evidence_signature_invalid(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    match &p.signature {
        // Real algorithms are accepted (judge cross-checks via the verifier).
        Some(s) if s.algo == "ed25519" => None,
        // Stub / hmac-stub / unsigned are all rejected in enforcement mode.
        Some(s) if s.algo == "stub" => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack signed with 'stub' algo; not acceptable in enforcement".into(),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        Some(s) if s.algo == "sha256-hmac-stub" => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack signed with HMAC stub; ed25519 required in enforcement".into(),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        Some(s) => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: format!("evidence pack signed with unknown algo '{}'", s.algo),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        None => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack is unsigned".into(),
            details: serde_json::Value::Null,
        }),
    }
}

fn cond_secret_scan_failed(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.secret_scan, ScanOutcome::Failed).then(|| HardStop {
        name: "secret_scan_failed".into(),
        reason: "gitleaks / secret scan reported findings".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_secret_scan_missing(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.secret_scan, ScanOutcome::Missing).then(|| HardStop {
        name: "secret_scan_missing".into(),
        reason: "secret scan never ran; fail-closed".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_sast_failed(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.sast, ScanOutcome::Failed).then(|| HardStop {
        name: "sast_failed".into(),
        reason: "SAST scan failed".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_dependency_scan_failed(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.dependency_scan, ScanOutcome::Failed).then(|| HardStop {
        name: "dependency_scan_failed".into(),
        reason: "dependency / cargo-deny scan failed".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_reviewer_blocked(_p: &EvidencePack, receipts: &[AgentApprovalReceipt]) -> Option<HardStop> {
    let blockers: Vec<&AgentApprovalReceipt> = receipts
        .iter()
        .filter(|r| matches!(r.decision, ReviewDecision::Block))
        .collect();
    if blockers.is_empty() {
        return None;
    }
    Some(HardStop {
        name: "reviewer_blocked".into(),
        reason: format!("{} reviewer(s) issued a hard block", blockers.len()),
        details: serde_json::json!({
            "roles": blockers.iter().map(|r| r.role).collect::<Vec<_>>(),
            "agents": blockers.iter().map(|r| r.agent_id.clone()).collect::<Vec<_>>(),
        }),
    })
}

fn cond_reviewer_abstained_required(
    _p: &EvidencePack,
    receipts: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    // Caller passes the required roles; without context we just look for any
    // abstain. The judge supplies refined logic via approvals.yml quorum.
    let any = receipts
        .iter()
        .any(|r| matches!(r.decision, ReviewDecision::Abstain));
    any.then(|| HardStop {
        name: "reviewer_abstained_required".into(),
        reason: "a required reviewer abstained; fail-closed unless explicit policy override".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_lockfile_only_change(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    p.supply_chain.lockfile_only_change.then(|| HardStop {
        name: "lockfile_only_change".into(),
        reason: "lockfile changed without source change (yanked-package backdoor pattern)".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_prompt_injection_suspected(
    _p: &EvidencePack,
    receipts: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    // Heuristic: a reviewer flags `class: "prompt-injection"`.
    let hits: Vec<&AgentApprovalReceipt> = receipts
        .iter()
        .filter(|r| {
            r.findings
                .iter()
                .any(|f| f.class.starts_with("prompt-injection"))
        })
        .collect();
    (!hits.is_empty()).then(|| HardStop {
        name: "prompt_injection_suspected".into(),
        reason: format!(
            "{} reviewer(s) flagged prompt-injection-class finding",
            hits.len()
        ),
        details: serde_json::Value::Null,
    })
}

/// Condition that depends on context the registry doesn't have at fusion time.
/// The judge / orchestrator pre-evaluates these and injects them by name into
/// `triggered_conditions`, OR leaves them out. Returning None here means "I
/// don't fire from pack+receipts alone; caller must inject me if I should."
fn cond_externally_supplied(_p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    None
}

// ---------------------------------------------------------------------------
// Anti-vibe deterministic detectors (Wave 2.1)
// ---------------------------------------------------------------------------
// These read only EvidencePack fields. They never call out to the network,
// filesystem, git, or an LLM. Each fires a hard stop when a known anti-vibe
// pattern shows up in the diff metadata. Detectors that need diff *content*
// (deleted assertions, @ts-ignore introductions, silent catch blocks) will
// live in a separate module once ChangedFile carries hunks.

const TEST_PATH_SUBSTRINGS: &[&str] = &[
    "/tests/",
    "/test/",
    "/__tests__/",
    "/spec/",
    ".test.",
    "_test.",
    ".spec.",
    "_spec.",
];

const SECURITY_SCANNER_CONFIG_PATHS: &[&str] = &[
    "deny.toml",
    "cargo-deny.toml",
    "audit.toml",
    ".trivyignore",
    ".gitleaks.toml",
    "gitleaks.toml",
    ".bandit",
    ".semgrep.yml",
    "agent/security-policy.toml",
];

const SECURITY_SCANNER_PATH_PREFIXES: &[&str] = &[
    ".github/workflows/security",
    ".gitlab/security-policies",
    "agent/security-policies",
];

const RELEASE_DEPLOY_POLICY_PATHS: &[&str] = &[
    ".autonomy/policies/release.yml",
    ".autonomy/policies/freeze.yml",
    "agent/proof-lanes.toml",
    "proof-lanes.toml",
    "Justfile",
];

const RELEASE_DEPLOY_PATH_PREFIXES: &[&str] = &[
    ".github/workflows/release",
    ".github/workflows/deploy",
    ".gitlab/ci/",
    "ops/ci/",
    "deploy/",
    "infra/",
    "k8s/",
    "helm/",
    "terraform/",
];

const PROMPT_OR_JUDGE_PREFIXES: &[&str] = &[
    ".autonomy/prompts/",
    ".autonomy/agents/",
    ".autonomy/policies/approvals.yml",
    ".autonomy/policies/risk.yml",
    ".autonomy/policies/protected-paths.yml",
];

const SECRET_HANDLING_PREFIXES: &[&str] = &["secrets/", "src/secrets", "src/llm/secrets"];

const LOCKFILE_BY_MANIFEST: &[(&str, &str)] = &[
    ("Cargo.lock", "Cargo.toml"),
    ("package-lock.json", "package.json"),
    ("yarn.lock", "package.json"),
    ("pnpm-lock.yaml", "package.json"),
    ("Pipfile.lock", "Pipfile"),
    ("poetry.lock", "pyproject.toml"),
    ("go.sum", "go.mod"),
    ("composer.lock", "composer.json"),
    ("Gemfile.lock", "Gemfile"),
];

fn is_test_path(p: &str) -> bool {
    TEST_PATH_SUBSTRINGS.iter().any(|s| p.contains(s))
}

fn any_path_matches(p: &EvidencePack, exact: &[&str], prefixes: &[&str]) -> Option<Vec<String>> {
    let hits: Vec<String> = p
        .changed_files
        .iter()
        .filter(|f| {
            exact.iter().any(|e| f.path == *e)
                || prefixes.iter().any(|prefix| f.path.starts_with(prefix))
        })
        .map(|f| f.path.clone())
        .collect();
    (!hits.is_empty()).then_some(hits)
}

fn cond_removes_or_weakens_tests(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    // Heuristic: a test file with lines_removed > lines_added is a net deletion;
    // multiple such files in one MR is a red flag absent a matching addition.
    let deletions: Vec<&crate::autonomy::types::ChangedFile> = p
        .changed_files
        .iter()
        .filter(|f| is_test_path(&f.path) && f.lines_removed > f.lines_added)
        .collect();
    if deletions.is_empty() {
        return None;
    }
    let total_removed: u32 = deletions
        .iter()
        .map(|f| f.lines_removed - f.lines_added)
        .sum();
    // Tolerate one small refactor; flag if either ≥2 deletion files or ≥20 net lines gone.
    if deletions.len() < 2 && total_removed < 20 {
        return None;
    }
    Some(HardStop {
        name: "removes_or_weakens_tests".into(),
        reason: format!(
            "{} test file(s) lost a net {} line(s); fail-closed without an explicit \
             test-integrity reviewer pass",
            deletions.len(),
            total_removed
        ),
        details: serde_json::json!({
            "paths": deletions.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            "net_lines_removed": total_removed,
        }),
    })
}

fn cond_coverage_threshold_lowered(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let delta = p.tests.coverage_delta?;
    // Anything more than a 0.5 percentage-point drop trips the alarm.
    if delta < -0.5 {
        Some(HardStop {
            name: "coverage_threshold_lowered".into(),
            reason: format!("coverage dropped {:.2}pp on this change", delta),
            details: serde_json::json!({ "coverage_delta": delta }),
        })
    } else {
        None
    }
}

fn cond_snapshot_mass_replacement(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let mut snap_lines: u32 = 0;
    let mut snap_files: Vec<String> = vec![];
    for f in &p.changed_files {
        let is_snap = f.path.contains("__snapshots__/")
            || f.path.contains("/snapshots/")
            || f.path.ends_with(".snap")
            || f.path.ends_with(".snap.json")
            || f.path.ends_with(".snap.new");
        if is_snap {
            snap_lines = snap_lines.saturating_add(f.lines_added + f.lines_removed);
            snap_files.push(f.path.clone());
        }
    }
    if snap_lines < 200 {
        return None;
    }
    Some(HardStop {
        name: "snapshot_mass_replacement".into(),
        reason: format!(
            "{} snapshot file(s) changed by {} line(s); needs explicit rendered-diff justification",
            snap_files.len(),
            snap_lines
        ),
        details: serde_json::json!({ "paths": snap_files, "lines": snap_lines }),
    })
}

fn cond_changes_security_scanner_config(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    any_path_matches(
        p,
        SECURITY_SCANNER_CONFIG_PATHS,
        SECURITY_SCANNER_PATH_PREFIXES,
    )
    .map(|paths| HardStop {
        name: "changes_security_scanner_config".into(),
        reason: "MR edits security scanner config; require elevated review".into(),
        details: serde_json::json!({ "paths": paths }),
    })
}

fn cond_changes_release_or_deploy_policy(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    any_path_matches(p, RELEASE_DEPLOY_POLICY_PATHS, RELEASE_DEPLOY_PATH_PREFIXES).map(|paths| {
        HardStop {
            name: "changes_release_or_deploy_policy".into(),
            reason: "MR edits release/deploy policy or infra; require elevated review".into(),
            details: serde_json::json!({ "paths": paths }),
        }
    })
}

fn cond_changes_agent_prompts_or_judge_policy(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    any_path_matches(p, &[], PROMPT_OR_JUDGE_PREFIXES).map(|paths| HardStop {
        name: "changes_agent_prompts_or_judge_policy".into(),
        reason: "MR edits agent prompts or judge policy; require elevated review (Law 3)".into(),
        details: serde_json::json!({ "paths": paths }),
    })
}

fn cond_touches_secret_handling(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    any_path_matches(p, &[".env", ".env.local"], SECRET_HANDLING_PREFIXES).map(|paths| HardStop {
        name: "touches_secret_handling".into(),
        reason: "MR edits secret-handling code or config; require security review".into(),
        details: serde_json::json!({ "paths": paths }),
    })
}

fn cond_introduces_new_external_code_source(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    if p.supply_chain.external_code_sources.is_empty() {
        return None;
    }
    Some(HardStop {
        name: "introduces_new_external_code_source".into(),
        reason: format!(
            "{} new external code source(s) declared",
            p.supply_chain.external_code_sources.len()
        ),
        details: serde_json::json!({ "sources": p.supply_chain.external_code_sources }),
    })
}

fn cond_lockfile_diff_without_manifest_diff(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let paths: std::collections::HashSet<&str> =
        p.changed_files.iter().map(|f| f.path.as_str()).collect();
    let mut orphans: Vec<String> = vec![];
    for (lock, manifest) in LOCKFILE_BY_MANIFEST {
        let lock_touched = paths
            .iter()
            .any(|p| *p == *lock || p.ends_with(&format!("/{lock}")));
        if !lock_touched {
            continue;
        }
        let manifest_touched = paths
            .iter()
            .any(|p| *p == *manifest || p.ends_with(&format!("/{manifest}")));
        if !manifest_touched {
            orphans.push(lock.to_string());
        }
    }
    if orphans.is_empty() {
        return None;
    }
    Some(HardStop {
        name: "lockfile_diff_without_manifest_diff".into(),
        reason: format!(
            "lockfile(s) {:?} changed without matching manifest; classic yanked-package backdoor pattern",
            orphans
        ),
        details: serde_json::json!({ "lockfiles": orphans }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::*;
    use chrono::Utc;

    fn pack_with_security(sast: ScanOutcome, dep: ScanOutcome, sec: ScanOutcome) -> EvidencePack {
        EvidencePack {
            schema: SchemaTag::new(),
            id: "evp_xx".into(),
            intent_id: None,
            repo: "r".into(),
            source_branch: "s".into(),
            target_branch: "main".into(),
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            policy_sha: "c".repeat(40),
            author_agent: None,
            risk: RiskTier::R2,
            changed_files: vec![],
            claims: vec![],
            tests: TestsSection {
                targeted: vec![],
                full_required: false,
                skipped: vec![],
                coverage_delta: None,
            },
            security: SecuritySection {
                sast,
                dependency_scan: dep,
                secret_scan: sec,
            },
            supply_chain: SupplyChainSection::default(),
            rollback: RollbackSection {
                strategy: RollbackStrategy::RevertCommit,
                feature_flag: None,
                data_migration_reversible: Some(true),
            },
            legacy_receipts: vec![],
            evidence_digest: format!("sha256:{}", "0".repeat(64)),
            created_at: Utc::now(),
            signature: None,
        }
    }

    fn blocked_receipt() -> AgentApprovalReceipt {
        AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: "aar_x".into(),
            evidence_pack_id: "evp_xx".into(),
            role: ReviewerRole::Security,
            agent_id: "reviewer-security.v1".into(),
            prompt_sha: None,
            provider: None,
            model: None,
            temperature: None,
            seed: None,
            raw_response_sha: None,
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            decision: ReviewDecision::Block,
            reason: Some("sql injection".into()),
            findings: vec![],
            not_author: true,
            tokens: TokenCounts::default(),
            created_at: Utc::now(),
            signature: Signature::stub(),
        }
    }

    #[test]
    fn unknown_condition_fail_closes() {
        let reg = ConditionRegistry::default();
        let p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        let hits = reg.evaluate(&["does_not_exist".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].name.starts_with("unknown_condition:"));
    }

    #[test]
    fn secret_scan_failed_triggers() {
        let reg = ConditionRegistry::default();
        let p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Failed,
        );
        let hits = reg.evaluate(&["secret_scan_failed".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "secret_scan_failed");
    }

    #[test]
    fn one_blocking_reviewer_is_a_hard_stop() {
        let reg = ConditionRegistry::default();
        let p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        let hits = reg.evaluate(&["reviewer_blocked".into()], &p, &[blocked_receipt()]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "reviewer_blocked");
    }

    fn with_files(paths_and_lines: &[(&str, u32, u32)]) -> EvidencePack {
        let mut p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        p.changed_files = paths_and_lines
            .iter()
            .map(|(path, add, rem)| ChangedFile {
                path: (*path).into(),
                risk_tags: vec![],
                lines_added: *add,
                lines_removed: *rem,
            })
            .collect();
        p
    }

    #[test]
    fn removes_or_weakens_tests_fires_on_multiple_deletions() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[
            ("src/foo.rs", 30, 5),
            ("tests/foo_test.rs", 0, 40),
            ("src/foo/__tests__/bar.test.ts", 1, 20),
        ]);
        let hits = reg.evaluate(&["removes_or_weakens_tests".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "removes_or_weakens_tests");
    }

    #[test]
    fn removes_or_weakens_tests_tolerates_small_refactor() {
        let reg = ConditionRegistry::default();
        // One test file, tiny net deletion → no fire.
        let p = with_files(&[("tests/util_test.rs", 8, 12)]);
        let hits = reg.evaluate(&["removes_or_weakens_tests".into()], &p, &[]);
        assert!(
            hits.is_empty(),
            "small single-file refactor should not fire"
        );
    }

    #[test]
    fn coverage_threshold_lowered_fires_on_drop() {
        let reg = ConditionRegistry::default();
        let mut p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        p.tests.coverage_delta = Some(-3.5);
        let hits = reg.evaluate(&["coverage_threshold_lowered".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "coverage_threshold_lowered");

        p.tests.coverage_delta = Some(0.0);
        let hits = reg.evaluate(&["coverage_threshold_lowered".into()], &p, &[]);
        assert!(hits.is_empty());
    }

    #[test]
    fn snapshot_mass_replacement_fires_above_threshold() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[("src/__snapshots__/widget.snap", 150, 80)]);
        let hits = reg.evaluate(&["snapshot_mass_replacement".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "snapshot_mass_replacement");
    }

    #[test]
    fn changes_security_scanner_config_fires_on_deny_toml() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[("deny.toml", 3, 1)]);
        let hits = reg.evaluate(&["changes_security_scanner_config".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "changes_security_scanner_config");
    }

    #[test]
    fn changes_release_or_deploy_policy_fires_on_deploy_path() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[("deploy/prod/k8s.yaml", 5, 0)]);
        let hits = reg.evaluate(&["changes_release_or_deploy_policy".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn changes_agent_prompts_or_judge_policy_fires_on_prompt_edit() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[(".autonomy/prompts/reviewer-security.md", 10, 2)]);
        let hits = reg.evaluate(&["changes_agent_prompts_or_judge_policy".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "changes_agent_prompts_or_judge_policy");
    }

    #[test]
    fn touches_secret_handling_fires() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[("src/secrets/vault.rs", 12, 0)]);
        let hits = reg.evaluate(&["touches_secret_handling".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn introduces_new_external_code_source_fires() {
        let reg = ConditionRegistry::default();
        let mut p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        p.supply_chain.external_code_sources = vec!["https://gist.github.com/foo".into()];
        let hits = reg.evaluate(&["introduces_new_external_code_source".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn lockfile_diff_without_manifest_diff_fires() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[("Cargo.lock", 20, 5), ("src/foo.rs", 3, 1)]);
        let hits = reg.evaluate(&["lockfile_diff_without_manifest_diff".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "lockfile_diff_without_manifest_diff");
    }

    #[test]
    fn lockfile_with_matching_manifest_does_not_fire() {
        let reg = ConditionRegistry::default();
        let p = with_files(&[("Cargo.lock", 20, 5), ("Cargo.toml", 1, 1)]);
        let hits = reg.evaluate(&["lockfile_diff_without_manifest_diff".into()], &p, &[]);
        assert!(hits.is_empty(), "matching manifest must suppress the fire");
    }

    #[test]
    fn wave3_release_conditions_are_registered() {
        let reg = ConditionRegistry::default();
        for name in [
            "release_artifact_unsigned",
            "release_sbom_missing",
            "release_provenance_missing",
            "rollback_drill_failed",
        ] {
            assert!(
                reg.lookup(name).is_some(),
                "wave-3 condition `{name}` must be registered in ConditionRegistry::default()"
            );
        }
    }

    #[test]
    fn wave3_release_conditions_are_externally_supplied() {
        // Local evaluation must return zero hard stops — these only fire when
        // the orchestrator injects them via JudgeInputs.external_hard_stops.
        let reg = ConditionRegistry::default();
        let p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        for name in [
            "release_artifact_unsigned",
            "release_sbom_missing",
            "release_provenance_missing",
            "rollback_drill_failed",
        ] {
            let nc = reg.lookup(name).expect("registered above");
            assert!(
                (nc.func)(&p, &[]).is_none(),
                "{name} must be a no-op locally; orchestrator injects it"
            );
            // And via evaluate(), still zero stops because pack+receipts alone
            // can't fire an externally-supplied condition.
            let hits = reg.evaluate(&[name.to_string()], &p, &[]);
            assert!(
                hits.is_empty(),
                "{name} fired unexpectedly during local evaluation: {hits:?}"
            );
        }
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// Windows-style backslash separators are NOT path separators in the
    /// detector's view: the lockfile-without-manifest check requires a
    /// trailing-slash match (`/Cargo.lock`) and the test-path heuristic
    /// keys on forward slashes. A Windows-style path like
    /// `repo\Cargo.lock` does not satisfy either, so the cross-platform
    /// behavior is "no match, no fire" — important to document so
    /// repository normalization never silently changes the semantics.
    #[test]
    fn path_matcher_does_not_misfire_on_windows_style_separators() {
        let reg = ConditionRegistry::default();
        // The CI on Windows would normalize to forward-slash before
        // emitting an EvidencePack; we assert that an UN-normalized
        // backslash path does NOT trip the lockfile rule (since the
        // detector matches `Cargo.lock` exactly or `/Cargo.lock` suffix).
        let p = with_files(&[("repo\\Cargo.lock", 20, 5), ("repo\\src\\main.rs", 3, 1)]);
        let hits = reg.evaluate(&["lockfile_diff_without_manifest_diff".into()], &p, &[]);
        assert!(
            hits.is_empty(),
            "backslash paths must not match the unix-style lockfile rule; got: {hits:?}"
        );
        // Sanity check: the test-path heuristic does NOT rely on `/tests/`
        // alone — it also keys on `_test.`, `.spec.`, etc. A
        // backslash-separated path that still contains one of those
        // substrings is recognized; that's the documented behavior on
        // Windows-normalized inputs. We assert the *invariant* that
        // matching is purely substring-based and never panics on
        // backslash paths, regardless of whether it fires.
        let win = with_files(&[("repo\\tests\\foo_test.rs", 0, 100)]);
        // Does not panic; whether it fires depends on substring matching
        // (`_test.` is in the heuristic, so this WILL fire — and that's OK,
        // because the path normalizer upstream is expected to convert
        // backslashes before the pack reaches us).
        let _ = reg.evaluate(&["removes_or_weakens_tests".into()], &win, &[]);
    }

    /// Evaluating an empty `requested` list against any pack must return an
    /// empty Vec — zero work, zero false positives.
    #[test]
    fn empty_pack_request_list_returns_no_hits() {
        let reg = ConditionRegistry::default();
        let p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        let hits = reg.evaluate(&[], &p, &[]);
        assert!(hits.is_empty(), "empty request must produce zero hits");
    }

    /// A pack with all-tests-skipped (every entry in `skipped`, none in
    /// `targeted`) must not falsely trigger the test-related conditions
    /// from the registry. Specifically `removes_or_weakens_tests` keys on
    /// file-level deletions, NOT on the `skipped` field — so a pack with
    /// 100 entries in skipped but zero file deletions must yield 0 hits.
    #[test]
    fn pack_with_all_tests_skipped_does_not_trigger_removes_or_weakens() {
        let reg = ConditionRegistry::default();
        let mut p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        // Mark a bunch of tests as skipped, but do NOT delete any test files.
        p.tests.skipped = (0..50).map(|i| format!("test::skip_{i}")).collect();
        p.tests.targeted.clear();
        // No file deletions; `removes_or_weakens_tests` must stay quiet.
        let hits = reg.evaluate(&["removes_or_weakens_tests".into()], &p, &[]);
        assert!(
            hits.is_empty(),
            "skipped tests without file deletions must not fire; got: {hits:?}"
        );
    }

    #[test]
    fn clean_pack_no_hard_stops() {
        let reg = ConditionRegistry::default();
        let p = pack_with_security(
            ScanOutcome::Passed,
            ScanOutcome::Passed,
            ScanOutcome::Passed,
        );
        let asked: Vec<String> = reg.names().iter().map(|s| s.to_string()).collect();
        let hits = reg.evaluate(&asked, &p, &[]);
        // evidence_signature_invalid will fire because pack is unsigned in this test;
        // that's the correct fail-closed behavior.
        assert!(hits.iter().any(|h| h.name == "evidence_signature_invalid"));
        assert!(!hits.iter().any(|h| h.name == "secret_scan_failed"));
        assert!(!hits.iter().any(|h| h.name == "sast_failed"));
    }
}
