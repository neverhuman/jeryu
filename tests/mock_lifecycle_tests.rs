//! Owner: Offline lifecycle tests — exercises GitLab API interactions without
//!         a live GitLab instance, using `MockGitlabServer` from `tests/common`.
//! Proof: `cargo nextest run --test mock_lifecycle_tests`
//! Invariants:
//!   - All tests run with zero external infrastructure (no Docker, no GitLab).
//!   - Tests are deterministic: no polling loops, no sleep() calls.
//!   - Any new GitLab API method added to `GitlabClient` gets a companion case here.

mod common;

use common::mock_gitlab::MockGitlabServer;

// ---------------------------------------------------------------------------
// Runner CRUD
// ---------------------------------------------------------------------------

/// Create, pause, resume, and delete a runner via the mock API.
#[tokio::test]
async fn test_runner_crud_offline() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();

    // Create
    let runner = client
        .create_runner("test-runner", &["ci", "test"], true, "instance_type")
        .await
        .expect("create runner");
    assert!(runner.id > 0, "runner id should be positive");
    assert!(!runner.token.is_empty(), "runner token should be non-empty");

    // Pause
    client
        .set_runner_paused(runner.id, true)
        .await
        .expect("pause runner");
    {
        let s = mock.state.lock().unwrap();
        let r = s.runners.get(&runner.id).expect("runner in mock state");
        assert!(r.paused, "runner should be paused after set_runner_paused(true)");
    }

    // Resume
    client
        .set_runner_paused(runner.id, false)
        .await
        .expect("resume runner");
    {
        let s = mock.state.lock().unwrap();
        let r = s.runners.get(&runner.id).expect("runner in mock state");
        assert!(!r.paused, "runner should not be paused after set_runner_paused(false)");
    }

    // List managers — mock returns empty list
    let managers = client
        .list_runner_managers(runner.id)
        .await
        .expect("list managers");
    assert!(managers.is_empty(), "mock always returns empty managers list");

    // Delete
    client
        .delete_runner(runner.id)
        .await
        .expect("delete runner");
    {
        let s = mock.state.lock().unwrap();
        assert!(
            !s.runners.contains_key(&runner.id),
            "runner should be absent after delete"
        );
    }
}

// ---------------------------------------------------------------------------
// Token rotation
// ---------------------------------------------------------------------------

/// Reset a runner's authentication token and verify the client sees the new one.
#[tokio::test]
async fn test_token_rotation_offline() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();

    let runner = client
        .create_runner("rotation-test", &[], true, "instance_type")
        .await
        .expect("create runner");
    let original_token = runner.token.clone();

    let new_token = client
        .reset_runner_token(runner.id)
        .await
        .expect("reset runner token");

    assert_ne!(
        new_token, original_token,
        "rotated token must differ from original"
    );
    assert!(!new_token.is_empty(), "rotated token must be non-empty");

    // Mock state must reflect the new token.
    let stored = mock
        .runner_token(runner.id)
        .expect("runner present after rotation");
    assert_eq!(stored, new_token, "mock stored token must match returned token");
}

// ---------------------------------------------------------------------------
// Pipeline polling
// ---------------------------------------------------------------------------

/// Commit a `.gitlab-ci.yml` file, verify the mock auto-creates a pending
/// pipeline, then poll the pipeline's jobs and advance them to success.
#[tokio::test]
async fn test_pipeline_polling_offline() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();

    // Create a project.
    let project = client
        .create_project("pipeline-test")
        .await
        .expect("create project");
    assert!(project.id > 0);

    // Commit a CI file — mock auto-creates a pending pipeline.
    client
        .create_file(
            project.id,
            "main",
            ".gitlab-ci.yml",
            "test:\n  script: echo ok\n",
            "add CI",
        )
        .await
        .expect("commit CI file");

    // List pipelines — should have exactly one in "pending" state.
    let pipelines = client
        .list_pipelines(project.id, Some("main"))
        .await
        .expect("list pipelines");
    assert_eq!(pipelines.len(), 1, "expected exactly one auto-created pipeline");
    let pipeline = &pipelines[0];
    assert_eq!(pipeline.status, "pending");

    // List jobs for that pipeline — should have one job named "test_job".
    let jobs = client
        .list_pipeline_jobs(project.id, pipeline.id)
        .await
        .expect("list pipeline jobs");
    assert_eq!(jobs.len(), 1, "expected exactly one job");
    assert_eq!(jobs[0].name, "test_job");
    assert_eq!(jobs[0].status, "pending");

    // Bridges: always empty for a flat pipeline.
    let bridges = client
        .list_pipeline_bridges(project.id, pipeline.id)
        .await
        .expect("list pipeline bridges");
    assert!(bridges.is_empty());

    // Advance to success.
    mock.advance_jobs("success");

    let jobs_after = client
        .list_pipeline_jobs(project.id, pipeline.id)
        .await
        .expect("list pipeline jobs after advance");
    assert_eq!(jobs_after[0].status, "success");
}

// ---------------------------------------------------------------------------
// Job trace
// ---------------------------------------------------------------------------

/// Verify the trace endpoint returns the text we stored in the mock.
#[tokio::test]
async fn test_job_trace_offline() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();

    let project = client
        .create_project("trace-test")
        .await
        .expect("create project");

    let expected_trace = "Build passed. All green.\n";
    let (pipeline_id, job_id) =
        mock.add_pipeline(project.id, "main", "build", "success", expected_trace);

    // Sanity-check the pipeline shows up.
    let pipelines = client
        .list_pipelines(project.id, None)
        .await
        .expect("list pipelines");
    assert!(
        pipelines.iter().any(|p| p.id == pipeline_id),
        "pipeline should be in the list"
    );

    // Fetch the job trace.
    let trace = client
        .job_trace(project.id, job_id)
        .await
        .expect("job trace");
    assert_eq!(trace, expected_trace);
}

// ---------------------------------------------------------------------------
// Job retry
// ---------------------------------------------------------------------------

/// Fail a job, requeue it via `requeue_job`, and verify it goes back to pending.
#[tokio::test]
async fn test_job_retry_offline() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();

    let project = client
        .create_project("retry-test")
        .await
        .expect("create project");

    let (_pipeline_id, job_id) =
        mock.add_pipeline(project.id, "main", "flaky-test", "pending", "");

    // Mark the job as failed.
    mock.set_job_status(job_id, "failed");

    // Requeue.
    client
        .requeue_job(project.id, job_id)
        .await
        .expect("requeue job");

    // Verify the mock state reset back to pending.
    let s = mock.state.lock().unwrap();
    let job = s.jobs.get(&job_id).expect("job in mock state");
    assert_eq!(
        job.status, "pending",
        "job status should be 'pending' after requeue"
    );
}

// ---------------------------------------------------------------------------
// Full offline job cycle
// ---------------------------------------------------------------------------

/// End-to-end GitLab API flow without any live infrastructure:
///   1. Register a runner.
///   2. Create a project and commit a CI file (auto-creates pending pipeline).
///   3. Poll pipelines and jobs (pending → success).
///   4. Fetch job trace.
///   5. Delete the runner.
///
/// This mirrors the live `test_job_cycle` integration test but runs entirely
/// against the in-process mock.
#[tokio::test]
async fn test_job_cycle_offline() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();

    // 1. Register runner.
    let runner = client
        .create_runner("offline-runner", &["ci"], true, "instance_type")
        .await
        .expect("create runner");

    // 2. Create project + commit CI file.
    let project = client
        .create_project("offline-cycle-test")
        .await
        .expect("create project");

    client
        .create_file(
            project.id,
            "main",
            ".gitlab-ci.yml",
            "test:\n  script: echo 'God Mode Active'\n",
            "add CI",
        )
        .await
        .expect("commit CI file");

    // 3a. Poll pipelines — one pending pipeline expected.
    let pipelines = client
        .list_pipelines(project.id, Some("main"))
        .await
        .expect("list pipelines");
    assert_eq!(pipelines.len(), 1, "pipeline auto-created on CI commit");
    let pipeline = &pipelines[0];
    assert_eq!(pipeline.status, "pending");

    // 3b. Advance to success (simulates runner picking up the job).
    mock.advance_jobs("success");

    // 3c. Poll again — should now be success.
    let jobs = client
        .list_pipeline_jobs(project.id, pipeline.id)
        .await
        .expect("list pipeline jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "success");

    // 4. Fetch trace.
    let trace = client
        .job_trace(project.id, jobs[0].id)
        .await
        .expect("job trace");
    assert!(
        trace.contains("God Mode Active"),
        "trace should contain expected output; got: {trace:?}"
    );

    // 5. Rotate token, then delete runner.
    let new_token = client
        .reset_runner_token(runner.id)
        .await
        .expect("rotate token");
    assert_ne!(new_token, runner.token);

    client.delete_runner(runner.id).await.expect("delete runner");

    // Verify runner gone from mock state.
    assert!(
        mock.state.lock().unwrap().runners.is_empty(),
        "all runners should be deleted"
    );
}

// ---------------------------------------------------------------------------
// Multiple runners in same mock instance
// ---------------------------------------------------------------------------

/// Verify that two runners do not interfere with each other's tokens or state.
#[tokio::test]
async fn test_multiple_runners_isolated_offline() {
    let mock = MockGitlabServer::start().await;
    let client = mock.gitlab_client();

    let r1 = client
        .create_runner("runner-1", &["pool-a"], true, "instance_type")
        .await
        .expect("create runner 1");
    let r2 = client
        .create_runner("runner-2", &["pool-b"], true, "instance_type")
        .await
        .expect("create runner 2");

    assert_ne!(r1.id, r2.id, "runner IDs should be unique");
    assert_ne!(r1.token, r2.token, "runner tokens should be unique");

    // Pause r1 — r2 must remain unpaused.
    client
        .set_runner_paused(r1.id, true)
        .await
        .expect("pause runner 1");

    let s = mock.state.lock().unwrap();
    assert!(s.runners[&r1.id].paused, "r1 should be paused");
    assert!(!s.runners[&r2.id].paused, "r2 should not be paused");
}
