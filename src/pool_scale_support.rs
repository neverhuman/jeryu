use super::*;

pub(crate) fn manager_state_counts_as_active(state: &str) -> bool {
    matches!(state, "starting" | "online")
}

pub(crate) fn manager_has_running_container(
    manager: &Manager,
    running_container_ids: &BTreeSet<String>,
) -> bool {
    running_container_ids.contains(&manager.docker_container_id)
}

async fn start_manager(store: &Db, docker: &DockerCtl, pool: &Pool, pool_name: &str) -> Result<()> {
    let manager_id = uuid::Uuid::new_v4().to_string();
    let config_dir = config::runners_dir()
        .join(&manager_id)
        .display()
        .to_string();
    let manager_cache_dir = config::manager_cache_dir(&manager_id);
    let pool_cache_dir = config::pool_cache_root(pool_name);
    let pool_targets_dir = config::pool_cargo_targets_root(pool_name);
    let pool_sccache_dir = config::pool_cargo_sccache_dir(pool_name);

    fs::create_dir_all(&config_dir)
        .with_context(|| format!("creating config dir: {config_dir}"))?;
    fs::create_dir_all(&manager_cache_dir)
        .with_context(|| format!("creating cache dir: {}", manager_cache_dir.display()))?;
    fs::create_dir_all(&pool_targets_dir)
        .with_context(|| format!("creating pool targets dir: {}", pool_targets_dir.display()))?;
    fs::create_dir_all(&pool_sccache_dir)
        .with_context(|| format!("creating pool sccache dir: {}", pool_sccache_dir.display()))?;

    let gitlab_url = format!(
        "http://{}:{}",
        config::GITLAB_HOSTNAME,
        config::GITLAB_HTTP_PORT
    );
    let config_content = config::render_runner_config(
        pool_name,
        &manager_id,
        &gitlab_url,
        &pool.auth_token,
        &pool.executor,
        &pool_cache_dir.display().to_string(),
        pool.concurrent,
        pool.request_concurrency,
    );
    fs::write(format!("{config_dir}/config.toml"), &config_content)?;

    let container_id = docker
        .start_runner_manager(
            &manager_id,
            &config_dir,
            &manager_cache_dir.display().to_string(),
            &pool_cache_dir.display().to_string(),
            &pool.executor,
            None,
        )
        .await
        .with_context(|| format!("starting manager for pool '{pool_name}'"))?;

    let manager = Manager {
        id: manager_id.clone(),
        pool_name: pool_name.to_string(),
        docker_container_id: container_id,
        system_id: None,
        state: "starting".to_string(),
        config_dir,
        started_at: Some(chrono::Utc::now().to_rfc3339()),
        last_contact_at: None,
    };
    store.insert_manager(&manager).await?;

    info!(manager_id, pool = pool_name, "started new manager");
    Ok(())
}

pub(crate) async fn start_pool_manager(
    store: &Db,
    docker: &DockerCtl,
    pool: &Pool,
    pool_name: &str,
) -> Result<()> {
    start_manager(store, docker, pool, pool_name).await
}
