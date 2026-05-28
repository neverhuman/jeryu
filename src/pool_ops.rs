use super::pool_scale::{
    manager_state_counts_as_active, remove_manager_cache_dir, stop_manager_for_node,
};
use super::*;

/// Pause a pool in GitLab (stops accepting new jobs) but keeps managers alive.
pub async fn pause_pool(store: &Db, client: &GitlabClient, pool_name: &str) -> Result<()> {
    let pool = match store.get_pool(pool_name).await? {
        Some(pool) => pool,
        None => return Err(anyhow::anyhow!("pool '{}' not found", pool_name)),
    };

    client
        .set_runner_paused(pool.gitlab_runner_id, true)
        .await?;
    store.update_pool_paused(pool_name, true).await?; // allowlist: pool orchestration owns runner state

    info!(pool = pool_name, "paused pool");
    Ok(())
}

/// Resume a paused pool.
pub async fn resume_pool(store: &Db, client: &GitlabClient, pool_name: &str) -> Result<()> {
    let pool = match store.get_pool(pool_name).await? {
        Some(pool) => pool,
        None => return Err(anyhow::anyhow!("pool '{}' not found", pool_name)),
    };

    client
        .set_runner_paused(pool.gitlab_runner_id, false)
        .await?;
    store.update_pool_paused(pool_name, false).await?; // allowlist: pool orchestration owns runner state

    info!(pool = pool_name, "resumed pool");
    Ok(())
}

/// Drain a pool: pause in GitLab, then SIGQUIT all managers, wait for
/// current jobs to finish, then stop and remove all manager containers.
pub async fn drain_pool(
    store: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
    pool_name: &str,
) -> Result<()> {
    pause_pool(store, client, pool_name).await?;

    let managers = store.list_managers(Some(pool_name)).await?; // allowlist: pool orchestration owns runner state
    for m in &managers {
        if manager_state_counts_as_active(&m.state) {
            info!(manager_id = %m.id, "draining manager");
            store.update_manager_state(&m.id, "draining").await?; // allowlist: pool orchestration owns runner state

            stop_manager_for_node(docker, m, config::runner_shutdown_timeout_secs() as i64)
                .await
                .ok();
            if m.node_alias.is_none() {
                remove_manager_cache_dir(docker, &m.id).await;
            }
            store.update_manager_state(&m.id, "stopped").await?; // allowlist: pool orchestration owns runner state

            info!(manager_id = %m.id, "manager drained and stopped");
        }
    }

    wait_for_active_managers(store, pool_name, 0, Duration::from_secs(90)).await?;
    info!(pool = pool_name, "pool fully drained");
    Ok(())
}

/// Remove a pool after draining managers and deregistering the GitLab runner.
pub async fn delete_pool(
    store: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
    pool_name: &str,
) -> Result<()> {
    let pool = match store.get_pool(pool_name).await? {
        Some(pool) => pool,
        None => return Err(anyhow::anyhow!("pool '{}' not found", pool_name)),
    };

    drain_pool(store, docker, client, pool_name).await.ok();
    client.delete_runner(pool.gitlab_runner_id).await.ok();
    store.delete_pool(pool_name).await?; // allowlist: pool orchestration owns runner state
    Ok(())
}

/// Rotate the auth token for a pool.
pub async fn rotate_pool_token(
    store: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
    pool_name: &str,
) -> Result<String> {
    let pool = match store.get_pool(pool_name).await? {
        Some(pool) => pool,
        None => return Err(anyhow::anyhow!("pool '{}' not found", pool_name)),
    };

    let new_token = client.reset_runner_token(pool.gitlab_runner_id).await?;
    info!(pool = pool_name, "got new runner auth token");

    let managers = store.list_managers(Some(pool_name)).await?; // allowlist: pool orchestration owns runner state
    let gitlab_url = format!(
        "http://{}:{}",
        config::GITLAB_HOSTNAME,
        config::GITLAB_HTTP_PORT
    );
    for m in &managers {
        let config_content = config::render_runner_config(
            pool_name,
            &m.id,
            &gitlab_url,
            &new_token,
            &pool.executor,
            &config::pool_cache_root(pool_name).display().to_string(),
            pool.concurrent,
            pool.request_concurrency,
        );
        let config_path = format!("{}/config.toml", m.config_dir);
        fs::write(&config_path, &config_content)
            .with_context(|| format!("rewriting config for manager {}", m.id))?;

        if manager_state_counts_as_active(&m.state) {
            docker
                .reload_runner_config(&m.docker_container_id)
                .await
                .ok();
        }
    }

    store.update_pool_token(pool_name, &new_token).await?; // allowlist: pool orchestration owns runner state
    let expected = store.count_active_managers(pool_name).await?; // allowlist: pool orchestration owns runner state
    if expected > 0 {
        wait_for_active_managers(store, pool_name, expected, Duration::from_secs(90)).await?;
    }

    info!(
        pool = pool_name,
        "token rotation complete — all managers updated"
    );
    Ok(new_token)
}

pub(crate) async fn wait_for_active_managers(
    store: &Db,
    pool_name: &str,
    expected: i64,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let active = store.count_active_managers(pool_name).await?; // allowlist: pool orchestration owns runner state
        if active == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for pool '{}' active managers to reach {} (current={})",
                pool_name,
                expected,
                active
            );
        }
        sleep(Duration::from_secs(2)).await;
    }
}
