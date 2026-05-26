use anyhow::Result;
use jeryu::state::Pool;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

mod common;

static POOL_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn wait_for_managers(db: &jeryu::state::Db, pool_name: &str, expected: i64) -> Result<i64> {
    for _ in 0..20 {
        let active = db.count_active_managers(pool_name).await?;
        if active == expected {
            return Ok(active);
        }
        sleep(Duration::from_millis(250)).await;
    }

    db.count_active_managers(pool_name).await
}

async fn create_ephemeral_pool(
    client: &jeryu::gitlab_client::GitlabClient,
    db: &jeryu::state::Db,
) -> Result<(String, i64)> {
    let suffix = uuid::Uuid::new_v4()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    let pool_name = format!("jeryu-test-pool-{suffix}");
    let runner = client
        .create_runner(
            &format!("jeryu-{pool_name}"),
            &["pool-test"],
            true,
            "instance_type",
        )
        .await?;

    db.insert_pool(&Pool {
        name: pool_name.clone(),
        gitlab_runner_id: runner.id,
        auth_token: runner.token.clone(),
        tags: "pool-test".into(),
        executor: "docker".into(),
        min_warm: 0,
        max_managers: 2,
        concurrent: 2,
        request_concurrency: 1,
        paused: false,
        trust_tier: "trusted".into(),
        cluster_alias: None,
        backend_type: "docker".into(),
    })
    .await?;

    Ok((pool_name, runner.id))
}

async fn cleanup_ephemeral_pool(
    client: &jeryu::gitlab_client::GitlabClient,
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

#[tokio::test]
async fn test_pool_scale_up_down() -> Result<()> {
    let _guard = POOL_TEST_LOCK.lock().await;
    let Some(client) = common::skip_if_not_ready().await? else {
        println!("Skipping since GitLab is not ready.");
        return Ok(());
    };

    let docker = jeryu::docker::DockerCtl::connect()?;
    let db = jeryu::state::Db::open().await?;
    let (pool_name, runner_id) = create_ephemeral_pool(&client, &db).await?;

    // Scale to 1 first, then to 2.
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;
    assert_eq!(wait_for_managers(&db, &pool_name, 1).await?, 1);

    println!("Scaling to 2...");
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 2).await?;
    let active = wait_for_managers(&db, &pool_name, 2).await?;
    assert_eq!(active, 2, "Failed to scale pool up to 2");

    // Scale to 1
    println!("Scaling to 1...");
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;
    let active_2 = wait_for_managers(&db, &pool_name, 1).await?;
    assert_eq!(active_2, 1, "Failed to scale pool down to 1");

    jeryu::pool::drain_pool(&db, &docker, &client, &pool_name).await?;
    let active_after = wait_for_managers(&db, &pool_name, 0).await?;
    assert_eq!(active_after, 0, "Pool should be drained after scale test");

    cleanup_ephemeral_pool(&client, &db, &pool_name, runner_id).await;
    Ok(())
}

#[tokio::test]
async fn test_pool_pause_resume() -> Result<()> {
    let _guard = POOL_TEST_LOCK.lock().await;
    let Some(client) = common::skip_if_not_ready().await? else {
        return Ok(());
    };
    let db = jeryu::state::Db::open().await?;
    let (pool_name, runner_id) = create_ephemeral_pool(&client, &db).await?;

    println!("Testing Pool Pause");
    jeryu::pool::pause_pool(&db, &client, &pool_name).await?;

    let p = db.get_pool(&pool_name).await?.unwrap();
    assert!(p.paused, "Pool should be paused in db");

    println!("Testing Pool Resume");
    jeryu::pool::resume_pool(&db, &client, &pool_name).await?;
    let p_resumed = db.get_pool(&pool_name).await?.unwrap();
    assert!(!p_resumed.paused, "Pool should be resumed in db");

    cleanup_ephemeral_pool(&client, &db, &pool_name, runner_id).await;
    Ok(())
}

#[tokio::test]
async fn test_pool_token_rotation() -> Result<()> {
    let _guard = POOL_TEST_LOCK.lock().await;
    let Some(client) = common::skip_if_not_ready().await? else {
        return Ok(());
    };
    let docker = jeryu::docker::DockerCtl::connect()?;
    let db = jeryu::state::Db::open().await?;
    let (pool_name, runner_id) = create_ephemeral_pool(&client, &db).await?;

    // First ensure we have at least 1 manager online
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;

    let original_pool = db.get_pool(&pool_name).await?.unwrap();

    println!("Testing Token Rotation for pool: {}", pool_name);
    let new_token = jeryu::pool::rotate_pool_token(&db, &docker, &client, &pool_name).await?;

    let updated_pool = db.get_pool(&pool_name).await?.unwrap();
    assert_ne!(
        original_pool.auth_token, new_token,
        "Token should have changed"
    );
    assert_eq!(
        updated_pool.auth_token, new_token,
        "DB should reflect new token"
    );

    // The managers should still be up
    let active = db.count_active_managers(&pool_name).await?;
    assert!(
        active >= 1,
        "Managers should still be running after rotation and SIGHUP"
    );

    jeryu::pool::drain_pool(&db, &docker, &client, &pool_name).await?;
    let active_after = wait_for_managers(&db, &pool_name, 0).await?;
    assert_eq!(
        active_after, 0,
        "Pool should be drained after token rotation"
    );

    cleanup_ephemeral_pool(&client, &db, &pool_name, runner_id).await;
    Ok(())
}

#[tokio::test]
async fn test_pool_graceful_drain() -> Result<()> {
    let _guard = POOL_TEST_LOCK.lock().await;
    // This is essentially teardown for our test state
    let Some(client) = common::skip_if_not_ready().await? else {
        return Ok(());
    };
    let docker = jeryu::docker::DockerCtl::connect()?;
    let db = jeryu::state::Db::open().await?;
    let (pool_name, runner_id) = create_ephemeral_pool(&client, &db).await?;

    // Ensure there is something to drain
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;

    println!("Testing Graceful Drain for pool: {}", pool_name);
    jeryu::pool::drain_pool(&db, &docker, &client, &pool_name).await?;

    let active = wait_for_managers(&db, &pool_name, 0).await?;
    assert_eq!(active, 0, "Managers should be 0 after full drain");

    cleanup_ephemeral_pool(&client, &db, &pool_name, runner_id).await;
    Ok(())
}
