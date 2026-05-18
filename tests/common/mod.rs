#![allow(dead_code)]

use anyhow::Result;
use jeryu::config;
use jeryu::gitlab_client::{GitlabClient, Project};
use jeryu::state::Pool;
use std::sync::Once;

static TEST_TIMEOUT_OVERRIDE: Once = Once::new();

fn ensure_fast_pool_shutdown() {
    TEST_TIMEOUT_OVERRIDE.call_once(|| {
        if std::env::var_os("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS").is_none() {
            unsafe {
                std::env::set_var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS", "30");
            }
        }
    });
}

pub async fn create_ephemeral_pool(
    client: &GitlabClient,
    db: &jeryu::state::Db,
) -> anyhow::Result<(String, i64)> {
    ensure_fast_pool_shutdown();
    let suffix = uuid::Uuid::new_v4()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    let pool_name = format!("jeryu-e2e-pool-{suffix}");
    let runner = client
        .create_runner(
            &format!("jeryu-{pool_name}"),
            &["e2e-test"],
            true,
            "instance_type",
        )
        .await?;

    db.insert_pool(&Pool {
        name: pool_name.clone(),
        gitlab_runner_id: runner.id,
        auth_token: runner.token.clone(),
        tags: "e2e-test".into(),
        executor: "docker".into(),
        min_warm: 0,
        max_managers: 2,
        concurrent: 2,
        request_concurrency: 1,
        paused: false,
        trust_tier: "trusted".into(),
    })
    .await?;

    Ok((pool_name, runner.id))
}

pub async fn cleanup_ephemeral_pool(
    client: &GitlabClient,
    db: &jeryu::state::Db,
    pool_name: &str,
    runner_id: i64,
) {
    let docker = jeryu::docker::DockerCtl::connect().ok();
    if let Some(docker) = docker.as_ref() {
        let _ = jeryu::pool::drain_pool(db, docker, client, pool_name).await;
    }
    let _ = client.delete_runner(runner_id).await;
    let _ = db.delete_pool(pool_name).await;
}
pub fn load_client_or_skip() -> Result<GitlabClient> {
    ensure_fast_pool_shutdown();
    let env_path = config::env_file();
    dotenvy::from_path(&env_path).ok();

    let pat = std::env::var("GITLAB_PAT")?;
    let url = format!("http://localhost:{}", config::GITLAB_HTTP_PORT);
    Ok(GitlabClient::new(&url, Some(pat)))
}

pub async fn skip_if_not_ready() -> Result<Option<GitlabClient>> {
    ensure_fast_pool_shutdown();
    let client = match load_client_or_skip() {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    if !client.is_ready().await {
        return Ok(None);
    }

    // Even when GitLab is reachable, the test also needs the gitlab-runner
    // Docker image pulled locally. Skip if Docker isn't accessible OR the
    // image isn't present — otherwise the test fails with a Docker 404 that
    // looks like a regression but is really a missing test dependency.
    if jeryu::docker::DockerCtl::connect().is_err() {
        return Ok(None);
    }
    let image = jeryu::config::GITLAB_RUNNER_IMAGE;
    let probe = tokio::process::Command::new("docker")
        .args(["image", "inspect", "--format={{.Id}}", image])
        .output()
        .await;
    let image_present = probe.map(|o| o.status.success()).unwrap_or(false);
    if !image_present {
        eprintln!("test skipped: docker image {image} not present locally");
        return Ok(None);
    }

    Ok(Some(client))
}

#[allow(dead_code)]
pub async fn create_test_project(client: &GitlabClient, prefix: &str) -> Result<Project> {
    let repo_name = format!(
        "{}-{}",
        prefix,
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
    );
    client.create_project(&repo_name).await
}
