use super::*;
use crate::autonomy::types::RiskTier;

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
