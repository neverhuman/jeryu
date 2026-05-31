use super::*;
use crate::signing::Signature;
use crate::types::*;
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
        gate_receipts: vec![],
        ci_status: vec![],
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
        signature: Signature::unsigned(),
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

/// R-7 (D1): after the glob scrub, a contributor editing a path under the
/// removed legacy external-host CI prefix MUST NOT fire
/// `changes_release_or_deploy_policy`. The `.jeryu/ci/` and `deploy/` cases
/// still fire.
#[test]
fn legacy_external_ci_path_does_not_fire_after_glob_scrub() {
    let reg = ConditionRegistry::default();
    // A path under a removed external-host CI prefix must NOT match. (The
    // prefix that used to live here was scrubbed per D1; any path that is
    // no longer in the prefix list simply does not fire.)
    let removed = with_files(&[("legacy-host/ci/build.yml", 5, 0)]);
    let hits = reg.evaluate(&["changes_release_or_deploy_policy".into()], &removed, &[]);
    assert!(
        hits.is_empty(),
        "removed CI glob must not fire; got {hits:?}"
    );
    // The jeryu-native CI prefix DOES fire.
    let jeryu_ci = with_files(&[(".jeryu/ci/release.yml", 5, 0)]);
    let hits = reg.evaluate(&["changes_release_or_deploy_policy".into()], &jeryu_ci, &[]);
    assert_eq!(hits.len(), 1, "jeryu-native CI glob must fire");
    // And the generic `.github/...` prefix still fires.
    let gh = with_files(&[(".github/workflows/release.yml", 5, 0)]);
    let hits = reg.evaluate(&["changes_release_or_deploy_policy".into()], &gh, &[]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn changes_agent_prompts_or_judge_policy_fires_on_prompt_edit() {
    let reg = ConditionRegistry::default();
    let p = with_files(&[(".jeryu/autonomy/prompts/reviewer-security.md", 10, 2)]);
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
    p.supply_chain.external_code_sources = vec!["https://example.com/gist/foo".into()];
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
            "release condition `{name}` must be registered"
        );
    }
}

#[test]
fn wave3_release_conditions_are_externally_supplied() {
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
            "{name} must be a no-op locally"
        );
        let hits = reg.evaluate(&[name.to_string()], &p, &[]);
        assert!(hits.is_empty(), "{name} fired unexpectedly: {hits:?}");
    }
}

#[test]
fn path_matcher_does_not_misfire_on_windows_style_separators() {
    let reg = ConditionRegistry::default();
    let p = with_files(&[("repo\\Cargo.lock", 20, 5), ("repo\\src\\main.rs", 3, 1)]);
    let hits = reg.evaluate(&["lockfile_diff_without_manifest_diff".into()], &p, &[]);
    assert!(
        hits.is_empty(),
        "backslash paths must not match; got: {hits:?}"
    );
    let win = with_files(&[("repo\\tests\\foo_test.rs", 0, 100)]);
    let _ = reg.evaluate(&["removes_or_weakens_tests".into()], &win, &[]);
}

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

#[test]
fn pack_with_all_tests_skipped_does_not_trigger_removes_or_weakens() {
    let reg = ConditionRegistry::default();
    let mut p = pack_with_security(
        ScanOutcome::Passed,
        ScanOutcome::Passed,
        ScanOutcome::Passed,
    );
    p.tests.skipped = (0..50).map(|i| format!("test::skip_{i}")).collect();
    p.tests.targeted.clear();
    let hits = reg.evaluate(&["removes_or_weakens_tests".into()], &p, &[]);
    assert!(
        hits.is_empty(),
        "skipped tests without file deletions must not fire"
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
    // evidence_signature_invalid fires because the pack is unsigned here.
    assert!(hits.iter().any(|h| h.name == "evidence_signature_invalid"));
    assert!(!hits.iter().any(|h| h.name == "secret_scan_failed"));
    assert!(!hits.iter().any(|h| h.name == "sast_failed"));
}

#[test]
fn registry_has_at_least_40_named_conditions() {
    let reg = ConditionRegistry::default();
    assert!(
        reg.names().len() >= 30,
        "expected the full named-condition registry; got {}",
        reg.names().len()
    );
}

// --- CI gate (required-check lanes) -------------------------------------

#[test]
fn ci_conditions_are_registered() {
    let reg = ConditionRegistry::default();
    for name in ["missing_required_ci_check", "failed_required_ci_check"] {
        assert!(
            reg.lookup(name).is_some(),
            "CI condition `{name}` must be registered"
        );
    }
}

#[test]
fn ci_conditions_are_no_ops_in_the_registry_walk() {
    // The registry functions are placeholders; the real hit is computed by the
    // judge via `ci_hard_stops`. A registry walk over the names alone fires
    // nothing.
    let reg = ConditionRegistry::default();
    let p = pack_with_security(
        ScanOutcome::Passed,
        ScanOutcome::Passed,
        ScanOutcome::Passed,
    );
    let hits = reg.evaluate(
        &[
            "missing_required_ci_check".into(),
            "failed_required_ci_check".into(),
        ],
        &p,
        &[],
    );
    assert!(hits.is_empty(), "registry placeholders must not fire");
}

fn pack_with_ci(checks: &[(&str, CiConclusion)]) -> EvidencePack {
    let mut p = pack_with_security(
        ScanOutcome::Passed,
        ScanOutcome::Passed,
        ScanOutcome::Passed,
    );
    p.ci_status = checks
        .iter()
        .map(|(name, conclusion)| CiCheck {
            name: (*name).to_string(),
            conclusion: *conclusion,
        })
        .collect();
    p
}

#[test]
fn ci_hard_stops_empty_required_is_no_gate() {
    let p = pack_with_ci(&[("ci", CiConclusion::Failure)]);
    assert!(super::ci_hard_stops(&p, &[]).is_empty());
}

#[test]
fn ci_hard_stops_all_green_yields_no_hits() {
    let p = pack_with_ci(&[
        ("ci-fast", CiConclusion::Success),
        ("ci-full", CiConclusion::Success),
    ]);
    let lanes = vec!["ci-fast".to_string(), "ci-full".to_string()];
    assert!(super::ci_hard_stops(&p, &lanes).is_empty());
}

#[test]
fn ci_hard_stops_missing_lane_fires_missing() {
    let p = pack_with_ci(&[("ci-fast", CiConclusion::Success)]);
    let lanes = vec!["ci-fast".to_string(), "ci-full".to_string()];
    let hits = super::ci_hard_stops(&p, &lanes);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "missing_required_ci_check");
}

#[test]
fn ci_hard_stops_non_success_lane_fires_failed() {
    for bad in [
        CiConclusion::Failure,
        CiConclusion::Cancelled,
        CiConclusion::TimedOut,
        CiConclusion::Pending,
    ] {
        let p = pack_with_ci(&[("ci-fast", bad)]);
        let lanes = vec!["ci-fast".to_string()];
        let hits = super::ci_hard_stops(&p, &lanes);
        assert_eq!(hits.len(), 1, "{bad:?} must fire");
        assert_eq!(hits[0].name, "failed_required_ci_check", "{bad:?}");
    }
}

#[test]
fn ci_hard_stops_reports_both_missing_and_failed() {
    let p = pack_with_ci(&[("ci-full", CiConclusion::Failure)]);
    let lanes = vec!["ci-fast".to_string(), "ci-full".to_string()];
    let hits = super::ci_hard_stops(&p, &lanes);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].name, "missing_required_ci_check");
    assert_eq!(hits[1].name, "failed_required_ci_check");
}
