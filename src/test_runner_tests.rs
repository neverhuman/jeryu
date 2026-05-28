use crate::test_runner::{TestRunPriority, TestRunReason};
use crate::{
    release,
    test_runner::{TestRunOpts, plan_test_run, render_ephemeral_ci_yaml},
};

#[test]
fn routes_deploy_commands_without_runner_tags() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p veox-deploy".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.risk_class, "build");
    assert!(plan.timeout_secs >= 1200);
}

#[test]
fn routes_security_commands_without_runner_tags() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p dougx security-scan".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.risk_class, "untrusted");
    assert!(plan.timeout_secs >= 1800);
}

#[test]
fn defaults_to_default_routing_for_simple_commands_without_tags() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p veox-testctl".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.risk_class, "default");
    assert_eq!(plan.priority, TestRunPriority::Normal);
    assert_eq!(plan.reason, TestRunReason::General);
}

#[test]
fn urgent_reason_defaults_to_high_scheduler_priority() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p jeryu -- test_runner".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        timeout_secs: 600,
        reason: TestRunReason::TestFix,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.priority, TestRunPriority::High);
    assert_eq!(plan.reason, TestRunReason::TestFix);
}

#[test]
fn explicit_scheduler_override_wins_over_reason_default() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p jeryu -- test_runner".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        timeout_secs: 600,
        priority: Some(TestRunPriority::Override),
        reason: TestRunReason::CherryPick,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.priority, TestRunPriority::Override);
    assert_eq!(plan.reason, TestRunReason::CherryPick);
}

#[test]
fn ephemeral_ci_yaml_uses_isolated_clone_path() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p jeryu".to_string(),
        job_name: Some("smoke".to_string()),
        image: "rust:1.92.0".to_string(),
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    let yaml = render_ephemeral_ci_yaml(&plan);

    assert!(yaml.contains("GIT_STRATEGY: clone"));
    assert!(yaml.contains(
        "GIT_CLONE_PATH: \"$CI_BUILDS_DIR/$CI_PROJECT_PATH_SLUG-jeryu-$CI_PIPELINE_ID-$CI_JOB_ID\""
    ));
    assert!(yaml.contains("JERYU_SCHEDULER_PRIORITY: normal"));
    assert!(yaml.contains("JERYU_SCHEDULER_REASON: general"));
    assert!(yaml.contains("    - cargo test -p jeryu"));
}

#[test]
fn ephemeral_ci_yaml_is_untagged() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p jeryu".to_string(),
        job_name: Some("smoke".to_string()),
        image: "rust:1.92.0".to_string(),
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    let yaml = render_ephemeral_ci_yaml(&plan);
    assert!(!yaml.contains("tags:"));
}
