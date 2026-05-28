//! Owner: CI Test Runner subsystem
//! Proof: `cargo nextest run -p jeryu -- test_runner`
//! Invariants: Test execution preserves lane semantics and reports enough structure for VTI feedback.
//! Test Runner: Agent-friendly single-test execution via jeryu.
//!
//! Enables an agent to run a single test (or set of tests) through the
//! GitLab CI pipeline and get structured results back. Works by creating
//! a dynamic pipeline with just the requested test command.

use crate::release;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use crate::runner_scheduler::{
    SchedulerPriority as TestRunPriority, SchedulerReason as TestRunReason,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of a test run through the CI pipeline.
#[derive(Debug, Serialize, Deserialize)]
pub struct TestRunResult {
    pub pipeline_id: i64,
    pub job_id: Option<i64>,
    pub job_name: String,
    pub status: String,
    pub duration_secs: Option<f64>,
    pub trace_tail: String,
    pub passed: bool,
}

/// Planning metadata for a test run.
#[derive(Debug, Serialize, Deserialize)]
pub struct TestRunPlan {
    pub command: String,
    pub job_name: String,
    pub image: String,
    pub timeout_secs: u64,
    pub risk_class: String,
    pub priority: TestRunPriority,
    pub reason: TestRunReason,
    pub rationale: Vec<String>,
}

/// Options for running a test.
#[derive(Debug, Clone)]
pub struct TestRunOpts {
    pub project_id: i64,
    pub test_command: String,
    pub job_name: Option<String>,
    pub image: String,
    pub timeout_secs: u64,
    pub force: bool,
    pub commit_sha: String,
    pub priority: Option<TestRunPriority>,
    pub reason: TestRunReason,
}

impl Default for TestRunOpts {
    fn default() -> Self {
        Self {
            project_id: release::DEFAULT_RELEASE_PROJECT_ID,
            test_command: String::new(),
            job_name: None,
            image: "rust:1.92.0".to_string(),
            timeout_secs: 600,
            force: false,
            commit_sha: "latest".to_string(),
            priority: None,
            reason: TestRunReason::General,
        }
    }
}

/// Options for running multiple tests in parallel through CI pipelines.
#[derive(Debug, Clone)]
pub struct TestBatchOpts {
    pub project_id: i64,
    pub test_commands: Vec<String>,
    pub job_name_prefix: Option<String>,
    pub image: String,
    pub timeout_secs: u64,
    pub max_parallel: usize,
    pub force: bool,
    pub commit_sha: String,
    pub priority: Option<TestRunPriority>,
    pub reason: TestRunReason,
}

impl Default for TestBatchOpts {
    fn default() -> Self {
        Self {
            project_id: release::DEFAULT_RELEASE_PROJECT_ID,
            test_commands: Vec::new(),
            job_name_prefix: None,
            image: "rust:1.92.0".to_string(),
            timeout_secs: 600,
            max_parallel: 3,
            force: false,
            commit_sha: "latest".to_string(),
            priority: None,
            reason: TestRunReason::General,
        }
    }
}

/// A queued CI test submission. Unlike [`TestBatchOpts`], this can carry
/// different projects and priorities in one scheduling pass.
#[derive(Debug, Clone)]
pub struct TestSubmission {
    pub project_id: i64,
    pub test_command: String,
    pub job_name: Option<String>,
    pub image: String,
    pub timeout_secs: u64,
    pub force: bool,
    pub commit_sha: String,
    pub priority: Option<TestRunPriority>,
    pub reason: TestRunReason,
}

// ---------------------------------------------------------------------------
// Core API
// ---------------------------------------------------------------------------

pub fn plan_test_run(opts: &TestRunOpts) -> TestRunPlan {
    let job_name = match opts.job_name.clone() {
        Some(value) => value,
        None => "jeryu-test-run".to_string(),
    };
    let routing = routing::infer_test_routing(&opts.test_command);
    let reason = opts.reason;
    let priority = match opts.priority {
        Some(priority) => priority,
        None => reason.default_priority(),
    };
    let timeout_secs = if opts.timeout_secs == TestRunOpts::default().timeout_secs {
        routing.timeout_secs
    } else {
        opts.timeout_secs
    };

    TestRunPlan {
        command: opts.test_command.clone(),
        job_name,
        image: opts.image.clone(),
        timeout_secs,
        risk_class: routing.risk_class,
        priority,
        reason,
        rationale: routing.rationale,
    }
}

#[derive(Serialize)]
struct EphemeralCiConfig<'a> {
    stages: Vec<&'a str>,
    #[serde(flatten)]
    jobs: BTreeMap<String, EphemeralCiJob<'a>>,
}

#[derive(Serialize)]
struct EphemeralCiJob<'a> {
    stage: &'a str,
    image: &'a str,
    variables: EphemeralCiVariables<'a>,
    script: Vec<String>,
}

#[derive(Serialize)]
struct EphemeralCiVariables<'a> {
    #[serde(rename = "GIT_STRATEGY")]
    git_strategy: &'a str,
    #[serde(rename = "GIT_CLONE_PATH")]
    git_clone_path: &'a str,
    #[serde(rename = "JERYU_SCHEDULER_PRIORITY")]
    scheduler_priority: &'a str,
    #[serde(rename = "JERYU_SCHEDULER_REASON")]
    scheduler_reason: &'a str,
}

pub(crate) fn render_ephemeral_ci_yaml(plan: &TestRunPlan) -> String {
    let mut jobs = BTreeMap::new();
    jobs.insert(
        plan.job_name.clone(),
        EphemeralCiJob {
            stage: "test",
            image: &plan.image,
            variables: EphemeralCiVariables {
                git_strategy: "clone",
                git_clone_path: "$CI_BUILDS_DIR/$CI_PROJECT_PATH_SLUG-jeryu-$CI_PIPELINE_ID-$CI_JOB_ID",
                scheduler_priority: plan.priority.label(),
                scheduler_reason: plan.reason.label(),
            },
            script: vec![plan.command.clone()],
        },
    );

    let yaml = serde_yaml::to_string(&EphemeralCiConfig {
        stages: vec!["test"],
        jobs,
    })
    .expect("serialize ephemeral CI yaml");

    format!("# Auto-generated by jeryu test run - ephemeral pipeline\n{yaml}")
}

#[path = "test_runner_routing.rs"]
mod routing;

#[path = "test_runner_runtime.rs"]
mod runtime;
pub async fn run_test(
    db: &crate::state::Db,
    client: &crate::gitlab_client::GitlabClient,
    opts: &TestRunOpts,
) -> Result<TestRunResult> {
    runtime::run_test(db, client, opts).await
}

pub(crate) async fn wait_for_test_result(
    client: &crate::gitlab_client::GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    job_name: &str,
    timeout_secs: u64,
) -> Result<TestRunResult> {
    runtime::wait_for_test_result(client, project_id, pipeline_id, job_name, timeout_secs).await
}

#[path = "test_runner_job_ops.rs"]
mod job_ops;
pub async fn run_test_batch(
    db: &crate::state::Db,
    client: &crate::gitlab_client::GitlabClient,
    opts: &TestBatchOpts,
) -> Result<Vec<TestRunResult>> {
    job_ops::run_test_batch(db, client, opts).await
}

pub async fn run_test_submissions(
    db: &crate::state::Db,
    client: &crate::gitlab_client::GitlabClient,
    submissions: Vec<TestSubmission>,
    max_parallel: usize,
) -> Result<Vec<TestRunResult>> {
    job_ops::run_test_submissions(db, client, submissions, max_parallel).await
}

pub async fn requeue_job_by_name(
    client: &crate::gitlab_client::GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    job_name: &str,
) -> Result<TestRunResult> {
    job_ops::requeue_job_by_name(client, project_id, pipeline_id, job_name).await
}

pub async fn pipeline_results(
    client: &crate::gitlab_client::GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<Vec<TestRunResult>> {
    job_ops::pipeline_results(client, project_id, pipeline_id).await
}

#[cfg(test)]
#[path = "test_runner_tests.rs"]
mod tests;
