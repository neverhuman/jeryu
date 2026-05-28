mod common;

use common::mock_gitlab::{MockGitlabServer, MockLintResponse};
use jeryu::{
    state::Db,
    test_runner::{TestRunOpts, run_test},
};

async fn open_test_db() -> Db {
    Db::open_url("sqlite::memory:")
        .await
        .expect("open in-memory test db")
}

#[tokio::test]
async fn lint_failure_stops_before_branch_creation() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();
    let db = open_test_db().await;
    let project = client
        .create_project("lint-preflight-test")
        .await
        .expect("create project");

    {
        let mut state = mock.state.lock().unwrap();
        state.lint_response = Some(MockLintResponse {
            valid: false,
            errors: vec!["jobs config should contain at least one visible job".to_string()],
            warnings: vec!["synthetic lint warning".to_string()],
            merged_yaml: None,
        });
    }

    let err = run_test(
        &db,
        &client,
        &TestRunOpts {
            project_id: project.id,
            test_command: "cargo test -p jeryu -- test_runner".to_string(),
            timeout_secs: 30,
            ..TestRunOpts::default()
        },
    )
    .await
    .expect_err("lint failure should stop the run before branch creation");

    let message = err.to_string();
    assert!(message.contains("generated test CI yaml failed GitLab lint"));
    assert!(message.contains("jobs config should contain at least one visible job"));
    assert!(message.contains("synthetic lint warning"));

    let state = mock.state.lock().unwrap();
    assert!(state.pipelines.is_empty(), "no pipeline should be created");
    assert!(state.jobs.is_empty(), "no jobs should be created");
}

#[tokio::test]
async fn run_test_selects_pipeline_matching_commit_sha() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();
    let db = open_test_db().await;
    let project = client
        .create_project("sha-selection-test")
        .await
        .expect("create project");

    {
        let mut state = mock.state.lock().unwrap();
        state.commit_job_name_override = Some("jeryu-test-run".to_string());
        state.commit_pipeline_status_override = Some("success".to_string());
    }

    let result = run_test(
        &db,
        &client,
        &TestRunOpts {
            project_id: project.id,
            test_command: "cargo test -p jeryu -- test_runner".to_string(),
            timeout_secs: 30,
            ..TestRunOpts::default()
        },
    )
    .await
    .expect("test run should succeed");

    assert!(
        result.passed,
        "selected pipeline should finish successfully"
    );
    assert_eq!(result.status, "success");
    assert_eq!(result.job_name, "jeryu-test-run");
    assert!(result.pipeline_id > 0);

    let jobs = client
        .list_pipeline_jobs(project.id, result.pipeline_id)
        .await
        .expect("list jobs for selected pipeline");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].name, "jeryu-test-run");
    assert_eq!(jobs[0].status, "success");
}

#[tokio::test]
async fn terminal_pipeline_without_target_job_reports_diagnostic_details() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();
    let db = open_test_db().await;
    let project = client
        .create_project("terminal-diagnostic-test")
        .await
        .expect("create project");

    {
        let mut state = mock.state.lock().unwrap();
        state.commit_job_name_override = Some("different-job".to_string());
        state.commit_pipeline_status_override = Some("success".to_string());
        state.commit_yaml_errors_override = Some("synthetic yaml error".to_string());
    }

    let result = run_test(
        &db,
        &client,
        &TestRunOpts {
            project_id: project.id,
            test_command: "cargo test -p jeryu -- test_runner".to_string(),
            timeout_secs: 30,
            ..TestRunOpts::default()
        },
    )
    .await
    .expect("run_test should return a structured terminal diagnostic");

    assert!(!result.passed);
    assert_eq!(result.status, "pipeline_success");
    assert!(result.job_id.is_none());
    assert!(
        result.trace_tail.contains("Pipeline ")
            && result
                .trace_tail
                .contains("reached terminal status 'success' before job 'jeryu-test-run' appeared")
    );
    assert!(
        result
            .trace_tail
            .contains("web_url: http://mock.gitlab.local/project/")
    );
    assert!(
        result
            .trace_tail
            .contains("yaml_errors: synthetic yaml error")
    );
    assert!(result.trace_tail.contains("No job trace was available."));
}
