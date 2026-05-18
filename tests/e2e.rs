use anyhow::Result;
use jeryu::config;
use std::time::Duration;
use tokio::time::sleep;

mod common;

// Helper to load client outside of main bin
fn load_client() -> Result<jeryu::gitlab_client::GitlabClient> {
    let env_path = config::env_file();
    dotenvy::from_path(&env_path).ok();

    let pat = std::env::var("GITLAB_PAT")?;
    let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    Ok(jeryu::gitlab_client::GitlabClient::new(&url, Some(pat)))
}

#[tokio::test]
async fn test_full_lifecycle() -> Result<()> {
    // 1. Setup
    let client = match load_client() {
        Ok(c) => c,
        Err(_) => {
            println!("Skipping E2E test — `jeryu bootstrap` hasn't been run.");
            return Ok(());
        }
    };

    // Skip if GitLab isn't up
    if !client.is_ready().await {
        println!("Skipping E2E test — GitLab is not running.");
        return Ok(());
    }

    // Skip if the gitlab-runner docker image isn't pulled locally — otherwise
    // the test fails with a 404 inside docker that looks like a regression.
    let image = jeryu::config::GITLAB_RUNNER_IMAGE;
    let image_present = tokio::process::Command::new("docker")
        .args(["image", "inspect", "--format={{.Id}}", image])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !image_present {
        println!("Skipping E2E test — docker image {image} not present locally.");
        return Ok(());
    }

    let docker = jeryu::docker::DockerCtl::connect()?;
    let db = jeryu::state::Db::open().await?;

    let repo_name = format!(
        "e2e-test-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
    );

    // 2. Create a dummy project & commit
    let project = client.create_project(&repo_name).await?;
    println!("Created project: {}", project.id);

    let ci_yaml = r#"
test_job:
  tags:
    - e2e-test
  script:
    - echo 'God Mode Active'
    - sleep 2
"#;

    client
        .commit_file(
            project.id,
            "main",
            ".gitlab-ci.yml",
            ci_yaml,
            "Initial commit pipeline",
            "create",
        )
        .await?;
    println!("Committed .gitlab-ci.yml");

    // 3. Scale test using ephemeral pool limits isolation!
    let (pool_name, runner_id) = common::create_ephemeral_pool(&client, &db).await?;
    println!(
        "Created ephemeral pool: {} (runner_id: {})",
        pool_name, runner_id
    );
    jeryu::pool::resume_pool(&db, &client, &pool_name).await?;
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;
    let active = db.count_active_managers(&pool_name).await?;
    assert!(active >= 1, "Failed to scale pool up");
    println!("Scaled ephemeral pool to {}", active);

    // 4. Wait for Pipeline
    let mut success = false;
    let mut trace_out = String::new();
    let mut job_id = 0;
    let mut pipeline_id = 0;
    let mut last_status = String::from("no job observed");
    let mut system_failure_retries = 0;

    println!("Waiting for pipeline to succeed...");
    for _ in 0..180 {
        sleep(Duration::from_secs(2)).await;
        let pipelines = match client.list_pipelines(project.id, Some("main")).await {
            Ok(p) => p,
            Err(e) => {
                println!("Transient error listing pipelines: {e}");
                continue;
            }
        };
        let Some(pipeline) = pipelines.first() else {
            continue;
        };
        pipeline_id = pipeline.id;

        let jobs = client.list_pipeline_jobs(project.id, pipeline.id).await?;
        if let Some(job) = jobs
            .iter()
            .filter(|job| job.name == "test_job")
            .max_by_key(|job| job.id)
        {
            job_id = job.id;
            last_status = job.status.clone();
            if job.status == "success" {
                trace_out = client.job_trace(project.id, job.id).await?;
                success = true;
                break;
            } else if job.status == "failed" {
                let trace = client
                    .job_trace(project.id, job.id)
                    .await
                    .unwrap_or_default();
                if system_failure_retries < 2
                    && (trace.contains("Job failed (system failure)")
                        || trace.contains("aborted: terminated"))
                {
                    system_failure_retries += 1;
                    println!(
                        "Retrying transient runner system failure for job {} ({}/2)",
                        job.id, system_failure_retries
                    );
                    jeryu::pool::drain_pool(&db, &docker, &client, &pool_name).await?;
                    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;
                    client.requeue_job(project.id, job.id).await?;
                    continue;
                }
                panic!("Job failed!\n{trace}");
            }
        }
    }

    assert!(
        success,
        "Pipeline {pipeline_id} did not succeed in time; last observed job status: {last_status}"
    );

    // 5. Verify Logs
    println!("Job {} succeeded. Checking trace...", job_id);
    assert!(
        trace_out.contains("God Mode Active"),
        "Trace missing expected output"
    );

    // 6. Drain test
    println!("Draining pool...");
    jeryu::pool::drain_pool(&db, &docker, &client, &pool_name).await?;

    let active_after = db.count_active_managers(&pool_name).await?;
    assert_eq!(active_after, 0, "Managers should be 0 after drain");

    common::cleanup_ephemeral_pool(&client, &db, &pool_name, runner_id).await;
    println!("E2E test passed.");
    Ok(())
}
