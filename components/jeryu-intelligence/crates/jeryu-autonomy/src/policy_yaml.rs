//! Strict-typed loaders for `.jeryu/autonomy/policies/*.yml`.
//!
//! YAML-only policy with named-condition references; no DSL. These loaders
//! accept only canonical policy keys so policy drift fails closed.

use crate::freeze::FreezeWindows;
use serde::Deserialize;

pub use crate::policy_yaml_types::*;

/// The full policy bundle the judge fuses against.
#[derive(Debug, Clone)]
pub struct PolicyBundle {
    pub risk: RiskPolicy,
    pub approvals: ApprovalsPolicy,
    pub release: ReleasePolicy,
    pub protected_paths: ProtectedPathsPolicy,
    /// Strict-typed freeze schedule (`vibegate.freeze.v1`). `None` when no
    /// `freeze.yml` is supplied — in which case no freeze enforcement runs.
    pub freeze: Option<FreezeWindows>,
}

impl PolicyBundle {
    /// Load every policy from in-memory YAML strings. (In the fused product the
    /// thin filesystem seam reads these off the protected target branch via
    /// the forge git layer; here the crate stays self-contained.)
    pub fn from_yaml_strs(
        risk: &str,
        approvals: &str,
        release: &str,
        protected_paths: &str,
        freeze: Option<&str>,
    ) -> Result<Self, String> {
        let risk: RiskPolicy = parse(risk, "risk.yml")?;
        let approvals: ApprovalsPolicy = parse(approvals, "approvals.yml")?;
        let release: ReleasePolicy = parse(release, "release.yml")?;
        let protected_paths: ProtectedPathsPolicy = parse(protected_paths, "protected-paths.yml")?;
        let freeze = match freeze {
            Some(s) => {
                Some(FreezeWindows::from_str_yaml(s).map_err(|e| format!("freeze.yml: {e}"))?)
            }
            None => None,
        };
        Ok(Self {
            risk,
            approvals,
            release,
            protected_paths,
            freeze,
        })
    }

    /// The quorum entry for a given tier, if the policy declares one.
    pub fn quorum_for(&self, tier: crate::types::RiskTier) -> Option<&QuorumEntry> {
        self.approvals.quorum.get(&tier)
    }
}

fn parse<T: for<'de> Deserialize<'de>>(s: &str, what: &str) -> Result<T, String> {
    serde_yaml::from_str(s).map_err(|e| format!("parse {what}: {e}"))
}

/// The canonical policy fixtures, embedded so the crate's tests and downstream
/// consumers have a faithful default bundle without a filesystem dependency.
/// These mirror `.jeryu/autonomy/policies/*.yml`.
pub mod fixtures {
    pub const RISK_YML: &str = r#"
schema: vibegate.risk.v1
tiers:
  - id: R5
    description: "missing/tampered evidence, suspicious behavior, unknown blast radius"
    matchers:
      - conditions: [evidence_missing]
      - conditions: [evidence_signature_invalid]
      - conditions: [prompt_injection_suspected]
      - conditions: [policy_sha_drift]
    auto_merge: false
    human_required: true
    fail_closed: true
  - id: R4
    description: "auth, crypto, secrets, infra, CI, policy, release, prod, prompt/judge rules"
    matchers:
      - any_path_matches_protected: true
    auto_merge: false
    human_required: true
  - id: R3
    description: "non-trivial logic, dependency changes, data migrations"
    matchers:
      - lines_changed_gte: 200
    auto_merge: false
    human_required: true
  - id: R2
    description: "moderate logic change with tests"
    matchers:
      - all_files_have_targeted_tests: true
    auto_merge: true
  - id: R1
    description: "small change with targeted tests"
    matchers:
      - max_lines_changed: 40
    auto_merge: true
  - id: R0
    description: "trivial / docs"
    matchers:
      - default: true
    auto_merge: true
"#;

    pub const APPROVALS_YML: &str = r#"
schema: vibegate.approvals.v1
invariants:
  no_self_approval: true
  exact_sha_required: true
  target_branch_policy_only: true
  fail_closed_on_missing_evidence: true
  fail_closed_on_agent_disagreement: true
  require_distinct_agent_identities: true
hard_stops:
  - name: secret_scan_failed
  - name: sast_failed
  - name: reviewer_blocked
  - name: sha_drift
  - name: policy_sha_drift
  - name: missing_required_review_role
  - name: missing_evidence_pack
  - name: evidence_signature_invalid
  - name: prompt_injection_suspected
  - name: codeowners_not_satisfied
  - name: freeze_window_active
  - name: budget_exceeded
  - name: training_use_required_but_disallowed
  - name: lockfile_diff_without_manifest_diff
  - name: judge_signature_invalid
quorum:
  R0: { approvals_needed: 0, roles: [], human_required: false }
  R1: { approvals_needed: 1, roles: [test_integrity], human_required: false }
  R2: { approvals_needed: 2, roles: [test_integrity, security], human_required: false }
  R3:
    approvals_needed: 4
    roles: [test_integrity, security, runtime, lockfile]
    human_required: true
  R4: { approvals_needed: 0, roles: [], human_required: true, fail_closed_without_human: true }
  R5: { approvals_needed: 0, roles: [], human_required: true, fail_closed: true }
verdict_ttl_minutes: 60
re_judge_on:
  - merge_train_rebase
  - target_branch_advance
  - policy_change_on_target
  - new_commit_on_pr
"#;

    pub const RELEASE_YML: &str = r#"
schema: vibegate.release.v1
build:
  build_once: true
  require_sbom: true
  require_slsa_provenance: true
  require_artifact_signature: true
  require_rollback_plan: true
release_ready_receipts:
  - build
  - sbom
  - provenance
"#;

    pub const PROTECTED_PATHS_YML: &str = r#"
schema: vibegate.protected_paths.v1
hard_human:
  - "src/auth/**"
  - "src/crypto/**"
  - "secrets/**"
  - ".jeryu/autonomy/**"
semantic_triggers:
  - touches_secret_handling
  - changes_security_scanner_config
"#;

    /// Build the canonical default bundle (no freeze window).
    pub fn default_bundle() -> super::PolicyBundle {
        super::PolicyBundle::from_yaml_strs(
            RISK_YML,
            APPROVALS_YML,
            RELEASE_YML,
            PROTECTED_PATHS_YML,
            None,
        )
        .expect("canonical fixtures parse")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bundle_loads() {
        let b = fixtures::default_bundle();
        assert_eq!(b.approvals.hard_stops.len(), 15);
        assert!(b.quorum_for(crate::types::RiskTier::R2).is_some());
        assert_eq!(
            b.quorum_for(crate::types::RiskTier::R2)
                .unwrap()
                .approvals_needed,
            2
        );
        assert!(
            b.quorum_for(crate::types::RiskTier::R4)
                .unwrap()
                .human_required
        );
    }

    #[test]
    fn default_bundle_has_no_required_ci_lanes() {
        // Back-compat: the canonical fixture declares no required CI lanes, so
        // the CI gate is off by default until a repo opts in.
        let b = fixtures::default_bundle();
        assert!(b.approvals.required_ci_lanes.is_empty());
    }

    #[test]
    fn required_ci_lanes_parse_from_approvals_yaml() {
        let approvals = r#"
schema: vibegate.approvals.v1
invariants: { no_self_approval: true }
required_ci_lanes: [ci-fast, ci-full]
quorum:
  R5: { approvals_needed: 0, roles: [], human_required: true, fail_closed: true }
hard_stops: []
"#;
        let b = PolicyBundle::from_yaml_strs(
            fixtures::RISK_YML,
            approvals,
            fixtures::RELEASE_YML,
            fixtures::PROTECTED_PATHS_YML,
            None,
        )
        .expect("parses with required_ci_lanes");
        assert_eq!(b.approvals.required_ci_lanes, vec!["ci-fast", "ci-full"]);
    }

    #[test]
    fn unknown_field_fails_closed() {
        let bad = "schema: vibegate.approvals.v1\ninvariants: {}\nquorum: {}\nbogus_key: true\n";
        let err = PolicyBundle::from_yaml_strs(
            fixtures::RISK_YML,
            bad,
            fixtures::RELEASE_YML,
            fixtures::PROTECTED_PATHS_YML,
            None,
        )
        .unwrap_err();
        assert!(err.contains("approvals.yml"), "got {err}");
    }
}
