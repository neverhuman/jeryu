//! Strict-typed loaders for `.jeryu/autonomy/policies/*.yml`.
//!
//! Decision #3: YAML-only policy with named-condition references; no DSL.
//! These loaders accept only canonical policy keys so policy drift fails closed.

use crate::autonomy::freeze::FreezeWindows;
use crate::autonomy::types::{ReviewerRole, RiskTier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// --- risk.yml ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RiskMatcher {
    #[serde(default)]
    pub paths_match: Vec<String>,
    #[serde(default)]
    pub paths_only_in: Vec<String>,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default)]
    pub max_lines_changed: Option<u32>,
    #[serde(default)]
    pub lines_changed_gte: Option<u32>,
    #[serde(default)]
    pub lines_changed_lte: Option<u32>,
    #[serde(default)]
    pub all_files_have_targeted_tests: Option<bool>,
    #[serde(default)]
    pub any_path_matches_protected: Option<bool>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskTierEntry {
    pub id: RiskTier,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub matchers: Vec<RiskMatcher>,
    #[serde(default)]
    pub auto_merge: bool,
    #[serde(default)]
    pub human_required: bool,
    #[serde(default)]
    pub fail_closed: bool,
    #[serde(default)]
    pub required_reviews: Vec<ReviewerRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskPolicy {
    pub schema: String,
    pub tiers: Vec<RiskTierEntry>,
    #[serde(default)]
    pub evaluation_order: Option<String>,
}

// --- approvals.yml -------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRules {
    #[serde(default = "true_default")]
    pub no_self_approval: bool,
    #[serde(default = "true_default")]
    pub exact_sha_required: bool,
    #[serde(default = "true_default")]
    pub target_branch_policy_only: bool,
    #[serde(default = "true_default")]
    pub fail_closed_on_missing_evidence: bool,
    #[serde(default = "true_default")]
    pub fail_closed_on_agent_disagreement: bool,
    #[serde(default = "true_default")]
    pub require_distinct_agent_identities: bool,
}

impl Default for ApprovalRules {
    fn default() -> Self {
        Self {
            no_self_approval: true,
            exact_sha_required: true,
            target_branch_policy_only: true,
            fail_closed_on_missing_evidence: true,
            fail_closed_on_agent_disagreement: true,
            require_distinct_agent_identities: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HardStopEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct QuorumEntry {
    #[serde(default)]
    pub approvals_needed: u32,
    #[serde(default)]
    pub roles: Vec<ReviewerRole>,
    #[serde(default)]
    pub human_required: bool,
    #[serde(default)]
    pub fail_closed: bool,
    #[serde(default)]
    pub fail_closed_without_human: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalsPolicy {
    pub schema: String,
    /// Required invariants for approval evaluation.
    pub invariants: ApprovalRules,
    #[serde(default)]
    pub hard_stops: Vec<HardStopEntry>,
    /// Per-tier quorum, keyed by `R0..R5`.
    pub quorum: HashMap<RiskTier, QuorumEntry>,
    #[serde(default)]
    pub verdict_ttl_minutes: Option<u32>,
    #[serde(default)]
    pub re_judge_on: Vec<String>,
}

// --- release.yml ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanaryRollbackOn {
    pub error_rate_relative_increase: f64,
    pub p95_latency_relative_increase: f64,
    pub crash_loop: bool,
    pub security_signal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanaryRules {
    #[serde(default)]
    pub initial_percent: u8,
    #[serde(default)]
    pub max_percent_without_human: u8,
    #[serde(default)]
    pub analysis_minutes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_on: Option<CanaryRollbackOn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NightwatchRules {
    pub may_rollback: bool,
    pub may_promote: bool,
    pub may_pause_pipeline: bool,
    pub may_page_human: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseBuildRules {
    pub build_once: bool,
    pub require_sbom: bool,
    pub require_slsa_provenance: bool,
    pub require_artifact_signature: bool,
    pub require_rollback_plan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePolicy {
    pub schema: String,
    pub build: ReleaseBuildRules,
    #[serde(default)]
    pub canary: Option<CanaryRules>,
    #[serde(default)]
    pub nightwatch: Option<NightwatchRules>,
    #[serde(default)]
    pub release_ready_receipts: Vec<String>,
}

// --- protected-paths.yml -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedPathsPolicy {
    pub schema: String,
    /// Paths whose change ALWAYS requires a human (R4 floor).
    pub hard_human: Vec<String>,
    /// Path-based semantic triggers (documentation; logic lives in conditions registry).
    #[serde(default)]
    pub semantic_triggers: Vec<String>,
}

// --- freeze.yml ----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FreezeRules {
    #[serde(default)]
    pub weekends: bool,
    #[serde(default)]
    pub dates: Vec<String>,
    #[serde(default)]
    pub hours: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreezePolicy {
    pub schema: String,
    #[serde(default)]
    pub freeze: FreezeRules,
}

// --- Bundle loader -------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PolicyBundle {
    pub risk: RiskPolicy,
    pub approvals: ApprovalsPolicy,
    pub release: ReleasePolicy,
    pub protected_paths: ProtectedPathsPolicy,
    /// Strict-typed freeze schedule (vibegate.freeze.v1). `None` when
    /// `.jeryu/autonomy/policies/freeze.yml` is missing — in which case no freeze
    /// enforcement runs, but operators see no error either.
    pub freeze: Option<FreezeWindows>,
}

impl PolicyBundle {
    pub fn from_dir(dir: &Path) -> std::io::Result<Self> {
        let risk: RiskPolicy = read_yaml(&dir.join("risk.yml"))?;
        let approvals: ApprovalsPolicy = read_yaml(&dir.join("approvals.yml"))?;
        let release: ReleasePolicy = read_yaml(&dir.join("release.yml"))?;
        let protected_paths: ProtectedPathsPolicy = read_yaml(&dir.join("protected-paths.yml"))?;
        let freeze_path = dir.join("freeze.yml");
        let freeze: Option<FreezeWindows> = if freeze_path.exists() {
            Some(FreezeWindows::from_path(&freeze_path)?)
        } else {
            None
        };
        Ok(Self {
            risk,
            approvals,
            release,
            protected_paths,
            freeze,
        })
    }
}

fn read_yaml<T: for<'de> Deserialize<'de>>(p: &Path) -> std::io::Result<T> {
    let s = std::fs::read_to_string(p)
        .map_err(|e| std::io::Error::new(e.kind(), format!("read {}: {e}", p.display())))?;
    serde_yaml::from_str(&s).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("parse {}: {e}", p.display()),
        )
    })
}

fn true_default() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_repo_autonomy_policies() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".jeryu/autonomy/policies");
        let bundle = PolicyBundle::from_dir(&dir).expect("loads policy bundle");
        assert_eq!(bundle.risk.schema, "vibegate.risk.v1");
        assert!(bundle.risk.tiers.iter().any(|t| t.id == RiskTier::R5));
        assert!(bundle.approvals.invariants.no_self_approval);
        assert!(bundle.release.build.build_once);
        assert!(bundle.release.build.require_sbom);
        assert!(bundle.release.build.require_slsa_provenance);
        assert!(bundle.release.build.require_artifact_signature);
        assert!(bundle.release.build.require_rollback_plan);
        assert!(
            bundle
                .release
                .release_ready_receipts
                .contains(&"proof-receipt".to_string())
        );
        assert!(!bundle.protected_paths.hard_human.is_empty());
        assert!(
            bundle
                .approvals
                .hard_stops
                .iter()
                .any(|h| h.name == "secret_scan_failed")
        );
        assert_eq!(
            bundle
                .approvals
                .quorum
                .get(&RiskTier::R2)
                .map(|q| q.approvals_needed),
            Some(2)
        );
    }

    #[test]
    fn approvals_quorum_round_trip() {
        let y = r#"
schema: vibegate.approvals.v1
invariants: { no_self_approval: true, exact_sha_required: true }
hard_stops:
  - { name: secret_scan_failed }
  - { name: reviewer_blocked }
quorum:
  R0: { approvals_needed: 0, roles: [], human_required: false }
  R2: { approvals_needed: 2, roles: [security, test_integrity], human_required: false }
  R4: { approvals_needed: 0, roles: [], human_required: true }
"#;
        let p: ApprovalsPolicy = serde_yaml::from_str(y).unwrap();
        assert_eq!(p.hard_stops.len(), 2);
        assert_eq!(p.quorum.get(&RiskTier::R2).unwrap().approvals_needed, 2);
        assert!(p.quorum.get(&RiskTier::R4).unwrap().human_required);
    }

    #[test]
    fn noncanonical_policy_keys_are_rejected() {
        let approvals = r#"
schema: vibegate.approvals.v1
rules: { no_self_approval: true }
hard_stops: []
quorum: {}
"#;
        assert!(serde_yaml::from_str::<ApprovalsPolicy>(approvals).is_err());

        let protected_paths = r#"
schema: vibegate.protected-paths.v1
paths: [".github/**"]
"#;
        assert!(serde_yaml::from_str::<ProtectedPathsPolicy>(protected_paths).is_err());

        let release = r#"
schema: vibegate.release.v1
build_once: true
require_sbom: true
require_slsa_provenance: true
require_artifact_signature: true
require_rollback_plan: true
"#;
        assert!(serde_yaml::from_str::<ReleasePolicy>(release).is_err());
    }
}
