use anyhow::Result;
use tokio::time::{Duration, sleep};

mod common;

#[tokio::test]
async fn test_job_cycle() -> Result<()> {
    let Some(client) = common::skip_if_not_ready().await? else {
        return Ok(());
    };
    let db = jeryu::state::Db::open().await?;
    let docker = jeryu::docker::DockerCtl::connect()?;

    let project = common::create_test_project(&client, "job-test").await?;
    let (pool_name, runner_id) = common::create_ephemeral_pool(&client, &db).await?;

    // Scale up an isolated runner so this test does not disturb live pools.
    jeryu::pool::resume_pool(&db, &client, &pool_name).await?;
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;

    let ci_yaml = r#"
test_retry:
  script:
    - echo 'First attempt'
    - exit 1
  rules:
    - if: $CI_PIPELINE_SOURCE == "push"
"#;

    client
        .commit_file(
            project.id,
            "main",
            ".gitlab-ci.yml",
            ci_yaml,
            "Pipeline to retry",
            "create",
        )
        .await?;

    // 1. Wait for failure on the expected pipeline/job.
    let mut job_id = 0;
    let mut pipeline_id = 0;
    let mut last_status = String::from("no job observed");
    println!("Waiting for job to fail...");
    // 60 × 2s = 120 s: give the runner time to start the container, run the
    // script, and report failure before we assert.
    for _ in 0..60 {
        sleep(Duration::from_secs(2)).await;
        let pipelines = client.list_pipelines(project.id, Some("main")).await?;
        let Some(pipeline) = pipelines.first() else {
            continue;
        };
        pipeline_id = pipeline.id;
        if let Some(job) = client
            .list_pipeline_jobs(project.id, pipeline.id)
            .await?
            .iter()
            .find(|job| job.name == "test_retry")
        {
            job_id = job.id;
            last_status = job.status.clone();
            if job.status == "failed" {
                break;
            }
            if job.status == "success" {
                panic!("Job succeeded unexpectedly before failure check");
            }
            continue;
        }
    }

    assert_ne!(
        job_id, 0,
        "Pipeline {pipeline_id} never produced the expected job; last observed status: {last_status}"
    );
    assert_eq!(
        last_status, "failed",
        "Job didn't hit failed state; last observed status: {last_status}"
    );

    // Fix it
    let ci_yaml_fix = r#"
test_retry:
  script:
    - echo 'Success attempt'
  rules:
    - if: $CI_PIPELINE_SOURCE == "push"
"#;
    println!("Pushing fix to pipeline");
    client
        .commit_file(
            project.id,
            "main",
            ".gitlab-ci.yml",
            ci_yaml_fix,
            "Fixing Pipeline",
            "update",
        )
        .await?;

    // We can't actually retry an old job using the *new* pipeline definition on GitLab,
    // "retry" reruns the *same* pipeline def. So instead, the new commit triggers a new job.
    // Let's test the retry API on the old job just to prove the API endpoint functions.
    println!("Calling retry API on failed job");
    client
        .requeue_job(project.id, job_id)
        .await
        .expect("Failed to call requeue_job API");

    // Now wait for the *new* pipeline job to succeed to test trace logic.
    println!("Waiting for new job to succeed");
    let mut trace_out = String::new();
    let mut success_job_id = 0;
    let mut last_new_job_status = String::from("no new job observed");
    for _ in 0..45 {
        sleep(Duration::from_secs(2)).await;
        let pipelines = client.list_pipelines(project.id, Some("main")).await?;
        for pipeline in pipelines {
            if pipeline.id == pipeline_id {
                continue;
            }
            for job in client.list_pipeline_jobs(project.id, pipeline.id).await? {
                if job.name != "test_retry" || job.id == job_id {
                    continue;
                }
                last_new_job_status = format!(
                    "pipeline {} job {} status {}",
                    pipeline.id, job.id, job.status
                );
                if job.status == "failed" {
                    let trace = client
                        .job_trace(project.id, job.id)
                        .await
                        .unwrap_or_default();
                    panic!(
                        "Fixed pipeline job failed unexpectedly; {last_new_job_status}; trace: {trace}"
                    );
                }
                if job.status == "success" {
                    success_job_id = job.id;
                    trace_out = client.job_trace(project.id, job.id).await?;
                    break;
                }
            }
            if success_job_id > 0 {
                break;
            }
        }
    }

    assert!(
        success_job_id > 0,
        "New pipeline did not succeed; last observed: {last_new_job_status}"
    );
    if !trace_out.contains("Success attempt") {
        for _ in 0..10 {
            sleep(Duration::from_secs(2)).await;
            trace_out = client.job_trace(project.id, success_job_id).await?;
            if trace_out.contains("Success attempt") {
                break;
            }
        }
    }
    assert!(
        trace_out.contains("Success attempt"),
        "Trace output was missing required logs"
    );

    // Clean up
    common::cleanup_ephemeral_pool(&client, &db, &pool_name, runner_id).await;

    Ok(())
}
