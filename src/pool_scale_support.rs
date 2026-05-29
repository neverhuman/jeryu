use super::*;
use crate::runner_backend::RunnerBackend;

pub(crate) fn manager_state_counts_as_active(state: &str) -> bool {
    // node_starting: remote SSH docker run issued; waiting for docker ps confirmation.
    // node_unreachable: SSH ping failed; container may still be running on remote node.
    // Both count as active to prevent the scale algorithm from spawning replacement
    // managers during a transient network outage (which would exceed max_managers).
    matches!(
        state,
        "starting" | "online" | "node_starting" | "node_unreachable"
    )
}

#[allow(dead_code)]
pub(crate) fn container_ids_match(left: &str, right: &str) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

/// Start a single new manager for `pool_name`, routing to local Docker or a
/// remote SSH node depending on the pool's `backend_type` and available nodes.
///
/// - `backend_type == "docker"` → local Docker (existing behaviour).
/// - `backend_type == "docker-remote"` → SSH node selected by least-loaded
///   algorithm; falls back to local Docker if no nodes are available.
pub(crate) async fn start_pool_manager(
    store: &Db,
    docker: &DockerCtl,
    pool: &Pool,
    pool_name: &str,
) -> Result<()> {
    if pool.backend_type == "docker-remote" {
        // Try to start on a remote node.
        if try_start_remote_manager(store, pool, pool_name)
            .await?
            .is_some()
        {
            return Ok(());
        }
        // Fall through to local Docker as a safety net.
        tracing::warn!(
            pool = pool_name,
            "no remote nodes available for docker-remote pool; falling back to local Docker"
        );
    }

    // Local Docker path (default / fallback).
    start_local_manager(store, docker, pool, pool_name).await
}

pub(crate) async fn start_pool_manager_on_node(
    store: &Db,
    docker: &DockerCtl,
    pool: &Pool,
    pool_name: &str,
    node_alias: Option<&str>,
) -> Result<()> {
    match node_alias {
        Some(alias) if alias != crate::config::LOCAL_NODE_ALIAS => {
            start_remote_manager_on_node(store, pool, pool_name, alias).await
        }
        _ => start_local_manager(store, docker, pool, pool_name).await,
    }
}

// ---------------------------------------------------------------------------
// Remote path
// ---------------------------------------------------------------------------

/// Try to start a manager on the best available remote node.
/// Returns `Ok(Some(()))` if a manager was started, `Ok(None)` if no node is available.
async fn try_start_remote_manager(store: &Db, pool: &Pool, pool_name: &str) -> Result<Option<()>> {
    use crate::node_support;

    // Build active-count map for node selection (counts managers currently on each node).
    let all_managers = store.list_managers(Some(pool_name)).await?;
    let mut active_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for m in &all_managers {
        if let Some(alias) = &m.node_alias
            && manager_state_counts_as_active(&m.state)
        {
            *active_counts.entry(alias.clone()).or_insert(0) += 1;
        }
    }

    let node_cfg = match node_support::select_node_for_pool(pool_name, &active_counts) {
        Some(n) => n,
        None => return Ok(None),
    };

    start_remote_manager(store, pool, pool_name, node_cfg).await?;
    Ok(Some(()))
}

pub(crate) async fn start_remote_manager_on_node(
    store: &Db,
    pool: &Pool,
    pool_name: &str,
    node_alias: &str,
) -> Result<()> {
    let node_cfg = crate::node_support::load_node_config(node_alias)?;
    if !node_cfg.enabled {
        anyhow::bail!("node '{node_alias}' is disabled");
    }
    if !node_cfg.accepts_pool(pool_name) {
        anyhow::bail!("node '{node_alias}' does not accept pool '{pool_name}'");
    }
    start_remote_manager(store, pool, pool_name, node_cfg).await
}

async fn start_remote_manager(
    store: &Db,
    pool: &Pool,
    pool_name: &str,
    node_cfg: crate::node_types::NodeConfig,
) -> Result<()> {
    use crate::runner_backend_remote::RemoteDockerBackend;

    let backend = RemoteDockerBackend::new(node_cfg.clone());
    let manager_id = uuid::Uuid::new_v4().to_string();

    let gitlab_url = node_cfg.gitlab_url_override.clone().unwrap_or_else(|| {
        format!(
            "http://{}:{}",
            crate::config::GITLAB_HOSTNAME,
            crate::config::GITLAB_HTTP_PORT
        )
    });

    let handle = backend
        .start_manager(pool_name, &manager_id, pool, &gitlab_url)
        .await
        .with_context(|| {
            format!(
                "starting remote manager for pool '{pool_name}' on node '{}'",
                node_cfg.alias
            )
        })?;

    let manager = Manager {
        id: manager_id.clone(),
        pool_name: pool_name.to_string(),
        docker_container_id: handle.backend_id,
        system_id: None,
        state: "node_starting".to_string(), // wait for reconciler to confirm it's up
        config_dir: handle.config_ref,
        started_at: Some(chrono::Utc::now().to_rfc3339()),
        last_contact_at: None,
        node_alias: Some(node_cfg.alias.clone()),
    };
    store.insert_manager(&manager).await?;

    tracing::info!(
        manager_id,
        pool = pool_name,
        node = %node_cfg.alias,
        "started remote runner manager"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Local path (original behaviour, unchanged)
// ---------------------------------------------------------------------------

pub(crate) async fn start_local_manager(
    store: &Db,
    docker: &DockerCtl,
    pool: &Pool,
    pool_name: &str,
) -> Result<()> {
    use std::fs;

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
        .start_runner_manager(crate::docker::RunnerManagerStartSpec {
            manager_id: &manager_id,
            pool_name,
            config_dir: &config_dir,
            manager_cache_dir: &manager_cache_dir.display().to_string(),
            pool_cache_dir: &pool_cache_dir.display().to_string(),
            executor: &pool.executor,
            docker_socket: None,
        })
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
        node_alias: None, // local Docker
    };
    store.insert_manager(&manager).await?;

    tracing::info!(manager_id, pool = pool_name, "started local manager");
    Ok(())
}

/// Stop a manager — routing to the appropriate backend based on its `node_alias`.
///
/// - `node_alias.is_none()` → local Docker stop/drain/rm
/// - `node_alias.is_some()` → SSH stop on the remote node
pub(crate) async fn stop_manager_for_node(
    docker: &DockerCtl,
    manager: &Manager,
    drain_timeout_secs: i64,
) -> Result<()> {
    if let Some(alias) = &manager.node_alias {
        // Remote manager: load NodeConfig, create backend, stop.
        match crate::node_support::load_node_config(alias) {
            Ok(node_cfg) => {
                let backend = crate::runner_backend_remote::RemoteDockerBackend::new(node_cfg);
                backend
                    .stop_manager(&manager.docker_container_id, drain_timeout_secs)
                    .await
                    .ok(); // best-effort
            }
            Err(e) => {
                tracing::warn!(
                    manager_id = %manager.id,
                    node = %alias,
                    error = %e,
                    "could not load node config for manager stop; skipping remote stop"
                );
            }
        }
    } else {
        // Local Docker stop.
        docker
            .cleanup_runner_cache(&manager.docker_container_id)
            .await
            .ok();
        docker
            .drain_runner_manager(&manager.docker_container_id, drain_timeout_secs)
            .await
            .ok();
        docker
            .cleanup_runner_cache(&manager.docker_container_id)
            .await
            .ok();
        docker
            .remove_runner_manager(&manager.docker_container_id)
            .await
            .ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::container_ids_match;

    #[test]
    fn container_ids_match_short_and_full_docker_ids() {
        let full = "ae04b80d94b0f3b0adba174f391d512bbdf0453f482b20a9913df8af693045a8";
        assert!(container_ids_match("ae04b80d94b0", full));
        assert!(container_ids_match(full, "ae04b80d94b0"));
        assert!(!container_ids_match("beefb80d94b0", full));
    }
}
