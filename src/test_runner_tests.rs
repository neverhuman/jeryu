use crate::{
    release,
    test_runner::{TestRunOpts, plan_test_run, render_ephemeral_ci_yaml},
};

#[test]
fn infers_build_routing_for_deploy_commands() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p veox-deploy".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        tags: None,
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.tags, vec!["build"]);
    assert_eq!(plan.risk_class, "build");
    assert!(plan.timeout_secs >= 1200);
}

#[test]
fn infers_untrusted_routing_for_security_commands() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p dougx security-scan".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        tags: None,
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.tags, vec!["untrusted"]);
    assert_eq!(plan.risk_class, "untrusted");
    assert!(plan.timeout_secs >= 1800);
}

#[test]
fn defaults_to_default_routing_for_simple_commands() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p veox-testctl".to_string(),
        job_name: None,
        image: "rust:1.92.0".to_string(),
        tags: None,
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    assert_eq!(plan.tags, vec!["default"]);
    assert_eq!(plan.risk_class, "default");
}

#[test]
fn ephemeral_ci_yaml_uses_isolated_clone_path() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p jeryu".to_string(),
        job_name: Some("smoke".to_string()),
        image: "rust:1.92.0".to_string(),
        tags: Some(vec!["build".to_string()]),
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    let yaml = render_ephemeral_ci_yaml(&plan);

    assert!(yaml.contains("GIT_STRATEGY: clone"));
    assert!(yaml.contains(
        "GIT_CLONE_PATH: \"$CI_BUILDS_DIR/$CI_PROJECT_PATH_SLUG-jeryu-$CI_PIPELINE_ID-$CI_JOB_ID\""
    ));
    assert!(yaml.contains("    - cargo test -p jeryu"));
}

#[test]
fn ephemeral_ci_yaml_is_untagged() {
    let plan = plan_test_run(&TestRunOpts {
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        test_command: "cargo test -p jeryu".to_string(),
        job_name: Some("smoke".to_string()),
        image: "rust:1.92.0".to_string(),
        tags: None,
        timeout_secs: 600,
        ..TestRunOpts::default()
    });

    let yaml = render_ephemeral_ci_yaml(&plan);
    assert!(!yaml.contains("tags:"));
}
