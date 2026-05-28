use crate::test_runner::{TestRunPriority, TestRunReason};
use crate::{
    release,
    test_runner::{TestRunOpts, plan_test_run, render_ephemeral_ci_yaml},
};

fn yaml_key(value: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(value.to_string())
}

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
    let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("rendered yaml should parse");
    let root = doc.as_mapping().expect("top-level yaml mapping");
    let stages = root
        .get(yaml_key("stages"))
        .and_then(|value| value.as_sequence())
        .expect("stages sequence");
    assert_eq!(stages[0].as_str(), Some("test"));
    let job = root
        .get(yaml_key("smoke"))
        .and_then(|value| value.as_mapping())
        .expect("job mapping");
    let variables = job
        .get(yaml_key("variables"))
        .and_then(|value| value.as_mapping())
        .expect("variables mapping");
    let script = job
        .get(yaml_key("script"))
        .and_then(|value| value.as_sequence())
        .expect("script sequence");

    assert_eq!(
        variables
            .get(yaml_key("GIT_STRATEGY"))
            .and_then(|value| value.as_str()),
        Some("clone")
    );
    assert_eq!(
        variables
            .get(yaml_key("GIT_CLONE_PATH"))
            .and_then(|value| value.as_str()),
        Some("$CI_BUILDS_DIR/$CI_PROJECT_PATH_SLUG-jeryu-$CI_PIPELINE_ID-$CI_JOB_ID")
    );
    assert_eq!(
        variables
            .get(yaml_key("JERYU_SCHEDULER_PRIORITY"))
            .and_then(|value| value.as_str()),
        Some("normal")
    );
    assert_eq!(
        variables
            .get(yaml_key("JERYU_SCHEDULER_REASON"))
            .and_then(|value| value.as_str()),
        Some("general")
    );
    assert_eq!(script[0].as_str(), Some("cargo test -p jeryu"));
}

#[test]
fn ephemeral_ci_yaml_keeps_yaml_sensitive_script_commands_as_strings() {
    for command in [
        "true",
        "false",
        "null",
        "yes",
        "echo foo: bar",
        "echo \"quoted\"",
        "echo foo # bar",
    ] {
        let plan = plan_test_run(&TestRunOpts {
            project_id: release::DEFAULT_RELEASE_PROJECT_ID,
            test_command: command.to_string(),
            job_name: Some("smoke".to_string()),
            image: "rust:1.92.0".to_string(),
            timeout_secs: 600,
            ..TestRunOpts::default()
        });

        let yaml = render_ephemeral_ci_yaml(&plan);
        let doc: serde_yaml::Value =
            serde_yaml::from_str(&yaml).expect("rendered yaml should parse");
        let root = doc.as_mapping().expect("top-level yaml mapping");
        let job = root
            .get(yaml_key("smoke"))
            .and_then(|value| value.as_mapping())
            .expect("job mapping");
        let script = job
            .get(yaml_key("script"))
            .and_then(|value| value.as_sequence())
            .expect("script sequence");

        assert_eq!(script.len(), 1, "script should contain exactly one item");
        assert_eq!(
            script[0].as_str(),
            Some(command),
            "command {command:?} should remain a YAML string"
        );
    }
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
