use super::*;
use crate::autonomy::kill_bell::KillBell;
use crate::autonomy::signing::EdSigningKey;
use crate::db::AnyPool;
use crate::db::autonomy_repo::fresh_autonomy_pool;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tempfile::TempDir;

async fn fresh_pool() -> AnyPool {
    fresh_autonomy_pool().await
}

fn make_autonomy_dir() -> TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    let root: PathBuf = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("policies")).unwrap();
    std::fs::create_dir_all(root.join("prompts")).unwrap();
    std::fs::write(
        root.join("policies/freeze.yml"),
        "schema: vibegate.freeze.v1\nenabled: false\nwindows: []\n",
    )
    .unwrap();
    std::fs::write(
        root.join("prompts/reviewer-nightwatch.md"),
        "# Nightwatch reviewer\n\nSystem prompt for the nightwatch reviewer.\n",
    )
    .unwrap();
    dir
}

fn fixed_now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-16T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn parse_recognizes_all_six_profiles() {
    for (s, expected) in [
        ("report_only", AutonomyProfile::ReportOnly),
        ("supervised", AutonomyProfile::Supervised),
        ("autonomous_merge", AutonomyProfile::AutonomousMerge),
        ("autonomous_release", AutonomyProfile::AutonomousRelease),
        ("sovereign", AutonomyProfile::Sovereign),
        ("sovereign_plus", AutonomyProfile::SovereignPlus),
    ] {
        let parsed = AutonomyProfile::parse(s).expect("known profile");
        assert_eq!(parsed, expected, "parse({s})");
        assert_eq!(parsed.name(), s, "name round-trip for {s}");
    }
    assert_eq!(
        AutonomyProfile::parse("  Sovereign_Plus  "),
        Some(AutonomyProfile::SovereignPlus)
    );
    assert!(AutonomyProfile::parse("does_not_exist").is_none());
}

#[tokio::test]
async fn validate_all_pass_returns_sovereign_plus() {
    let autonomy = make_autonomy_dir();
    let pool = fresh_pool().await;
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.98),
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(
        report.all_passed(),
        "report should be all-pass; failures: {:?}",
        report.failed
    );
    assert_eq!(report.effective_profile, AutonomyProfile::SovereignPlus);
    for required in [
        "kill_bell_armed",
        "freeze_check_wired",
        "nightwatch_prompt_present",
        "canary_default_rings",
        "rollback_drill_executor_available",
        "shadow_agreement_recent",
    ] {
        assert!(
            report.passed.iter().any(|p| p == required),
            "expected '{required}' in passed list; got {:?}",
            report.passed
        );
    }
}

#[tokio::test]
async fn validate_missing_freeze_yml_downgrades_to_sovereign() {
    let autonomy = make_autonomy_dir();
    std::fs::remove_file(autonomy.path().join("policies/freeze.yml")).unwrap();
    let pool = fresh_pool().await;
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.99),
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(!report.all_passed());
    assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
    let f = report
        .failed
        .iter()
        .find(|f| f.guardrail == "freeze_check_wired")
        .expect("freeze_check_wired must fail when file is missing");
    assert!(
        f.reason.contains("missing"),
        "reason should mention 'missing'; got {}",
        f.reason
    );
    assert!(!f.remediation.is_empty(), "every failure needs remediation");
}

#[tokio::test]
async fn validate_missing_nightwatch_prompt_fails() {
    let autonomy = make_autonomy_dir();
    std::fs::remove_file(autonomy.path().join("prompts/reviewer-nightwatch.md")).unwrap();
    let pool = fresh_pool().await;
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.99),
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(!report.all_passed());
    let f = report
        .failed
        .iter()
        .find(|f| f.guardrail == "nightwatch_prompt_present")
        .expect("nightwatch prompt missing must fail");
    assert!(
        f.reason.contains("reviewer-nightwatch.md"),
        "reason should mention the filename; got {}",
        f.reason
    );
    assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
}

#[tokio::test]
async fn validate_paused_kill_bell_fails() {
    let autonomy = make_autonomy_dir();
    let pool = fresh_pool().await;
    let key = EdSigningKey::generate("operator.test");
    let bell = KillBell::new(pool.clone());
    bell.pause("test pause", "alice", 3600, &key, fixed_now())
        .await
        .unwrap();
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.99),
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(!report.all_passed());
    let f = report
        .failed
        .iter()
        .find(|f| f.guardrail == "kill_bell_armed")
        .expect("paused bell must trip the kill_bell_armed check");
    assert!(f.reason.contains("alice"), "reason: {}", f.reason);
    assert!(f.reason.contains("test pause"), "reason: {}", f.reason);
    assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
}

#[tokio::test]
async fn validate_no_shadow_history_fails_with_message() {
    let autonomy = make_autonomy_dir();
    let pool = fresh_pool().await;
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: None,
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(!report.all_passed());
    let f = report
        .failed
        .iter()
        .find(|f| f.guardrail == "shadow_agreement_recent")
        .expect("missing shadow history must fail");
    assert!(
        f.reason.contains("no recent shadow report"),
        "reason should say 'no recent shadow report'; got {}",
        f.reason
    );
    assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
}

#[tokio::test]
async fn validate_low_shadow_agreement_fails() {
    let autonomy = make_autonomy_dir();
    let pool = fresh_pool().await;
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.80),
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(!report.all_passed());
    let f = report
        .failed
        .iter()
        .find(|f| f.guardrail == "shadow_agreement_recent")
        .expect("low agreement must fail");
    assert!(
        f.reason.contains("0.80") || f.reason.contains("0.8000"),
        "reason should include the observed rate; got {}",
        f.reason
    );
    assert!(
        f.reason.contains("0.95") || f.reason.contains("0.9500"),
        "reason should include the floor; got {}",
        f.reason
    );
    assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
}

#[tokio::test]
async fn validate_shadow_agreement_at_threshold_passes() {
    let autonomy = make_autonomy_dir();
    let pool = fresh_pool().await;
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.95),
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(
        report.all_passed(),
        "0.95 at floor 0.95 must pass; failures: {:?}",
        report.failed
    );
    assert_eq!(report.effective_profile, AutonomyProfile::SovereignPlus);
}

#[tokio::test]
async fn report_render_human_lists_each_failure_with_remediation() {
    let autonomy = make_autonomy_dir();
    std::fs::remove_file(autonomy.path().join("policies/freeze.yml")).unwrap();
    std::fs::remove_file(autonomy.path().join("prompts/reviewer-nightwatch.md")).unwrap();
    let pool = fresh_pool().await;
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: None,
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .unwrap();
    assert!(!report.all_passed());
    assert!(report.failed.len() >= 3);
    let rendered = report.render_human();
    for f in &report.failed {
        assert!(
            rendered.contains(&f.guardrail),
            "render should list guardrail '{}'; got:\n{rendered}",
            f.guardrail
        );
        assert!(
            rendered.contains(&f.remediation),
            "render should list remediation for '{}'; got:\n{rendered}",
            f.guardrail
        );
    }
    assert!(
        rendered.contains("effective profile: sovereign"),
        "render should state the effective profile; got:\n{rendered}"
    );
}

#[tokio::test]
async fn validate_with_partial_guardrails_disabled_skips_them() {
    let autonomy = make_autonomy_dir();
    let pool = fresh_pool().await;
    let guardrails = SovereignPlusGuardrails {
        require_nightwatch: false,
        require_canary: false,
        require_rollback_drill: false,
        require_kill_bell_armed: true,
        require_freeze_check: true,
        require_shadow_agreement_min: 0.0,
    };
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.0),
        },
        &guardrails,
    )
    .await
    .unwrap();
    for skipped in [
        "nightwatch_prompt_present",
        "canary_default_rings",
        "rollback_drill_executor_available",
    ] {
        assert!(
            !report.passed.iter().any(|s| s == skipped),
            "{skipped} must NOT be in passed when its toggle is false; passed: {:?}",
            report.passed
        );
        assert!(
            !report.failed.iter().any(|f| f.guardrail == skipped),
            "{skipped} must NOT be in failed when its toggle is false; failed: {:?}",
            report.failed
        );
    }
    assert!(report.passed.iter().any(|s| s == "kill_bell_armed"));
    assert!(report.passed.iter().any(|s| s == "freeze_check_wired"));
    assert!(report.passed.iter().any(|s| s == "shadow_agreement_recent"));
}

#[tokio::test]
async fn validate_extremely_high_shadow_agreement_floor_rejects_near_perfect() {
    let autonomy = make_autonomy_dir();
    let pool = fresh_pool().await;
    let guardrails = SovereignPlusGuardrails {
        require_shadow_agreement_min: 1.0,
        ..Default::default()
    };
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: autonomy.path(),
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.999),
        },
        &guardrails,
    )
    .await
    .unwrap();
    let f = report
        .failed
        .iter()
        .find(|f| f.guardrail == "shadow_agreement_recent")
        .expect("0.999 vs 1.0 floor must fail");
    assert!(
        f.reason.contains("required"),
        "reason should cite the floor"
    );
    assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
}

#[tokio::test]
async fn validate_missing_autonomy_dir_downgrades_with_clear_failures() {
    let pool = fresh_pool().await;
    let missing = std::path::PathBuf::from("/definitely/does/not/exist/jeryu-test-autonomy-d6c3");
    let report = validate_sovereign_plus(
        ValidatorInputs {
            autonomy_dir: &missing,
            ledger_pool: Some(&pool),
            now: fixed_now(),
            latest_shadow_agreement: Some(0.99),
        },
        &SovereignPlusGuardrails::default(),
    )
    .await
    .expect("validator must not Err on missing dir; it surfaces per-guardrail failures");
    assert!(!report.all_passed());
    assert!(
        report
            .failed
            .iter()
            .any(|f| f.guardrail == "freeze_check_wired"),
        "freeze_check_wired must fail when the dir is missing"
    );
    assert!(
        report
            .failed
            .iter()
            .any(|f| f.guardrail == "nightwatch_prompt_present"),
        "nightwatch_prompt_present must fail when the dir is missing"
    );
    assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
}

#[test]
fn report_all_passed_is_false_when_any_fails() {
    let report = GuardrailReport {
        passed: vec!["one".into(), "two".into()],
        failed: vec![GuardrailFailure {
            guardrail: "x".into(),
            reason: "y".into(),
            remediation: "z".into(),
        }],
        effective_profile: AutonomyProfile::Sovereign,
    };
    assert!(!report.all_passed());

    let report = GuardrailReport {
        passed: vec!["one".into()],
        failed: vec![],
        effective_profile: AutonomyProfile::SovereignPlus,
    };
    assert!(report.all_passed());
}
