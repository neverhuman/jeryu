use super::*;

impl DockerCtl {
    /// Drain a runner manager: SIGQUIT then wait for exit.
    pub async fn drain_runner_manager(&self, container_id: &str, timeout_secs: i64) -> Result<()> {
        info!(container_id, "sending SIGQUIT to drain runner manager");
        self.docker
            .kill_container(
                container_id,
                Some(KillContainerOptions { signal: "SIGQUIT" }),
            )
            .await
            .context("sending SIGQUIT to runner manager")?;

        debug!(container_id, timeout_secs, "waiting for runner to drain");
        let _ = self
            .docker
            .stop_container(container_id, Some(StopContainerOptions { t: timeout_secs }))
            .await;
        Ok(())
    }

    /// Force-stop a runner manager.
    pub async fn stop_runner_manager(&self, container_id: &str) -> Result<()> {
        self.docker
            .stop_container(container_id, Some(StopContainerOptions { t: 10 }))
            .await
            .context("stopping runner manager")?;
        info!(container_id, "stopped runner manager");
        Ok(())
    }

    /// Remove a stopped container.
    pub async fn remove_runner_manager(&self, container_id: &str) -> Result<()> {
        self.docker
            .remove_container(
                container_id,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .context("removing runner container")?;
        info!(container_id, "removed runner container");
        Ok(())
    }

    /// SIGHUP for runner config hot-reload.
    pub async fn reload_runner_config(&self, container_id: &str) -> Result<()> {
        self.docker
            .kill_container(
                container_id,
                Some(KillContainerOptions { signal: "SIGHUP" }),
            )
            .await
            .context("sending SIGHUP to runner manager")?;
        debug!(container_id, "sent SIGHUP for config reload");
        Ok(())
    }

    /// Get recent logs from a manager container.
    pub async fn manager_logs(&self, container_id: &str, tail: usize) -> Result<Vec<String>> {
        let opts = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail.to_string(),
            ..Default::default()
        };

        let stream = self.docker.logs(container_id, Some(opts));
        let chunks: Vec<_> = stream.try_collect().await.context("reading runner logs")?;

        let lines: Vec<String> = chunks.iter().map(|c| c.to_string()).collect();
        Ok(lines)
    }

    /// List all jeryu-managed containers.
    pub async fn list_managed_containers(&self) -> Result<Vec<ContainerSummary>> {
        let mut filters = HashMap::new();
        filters.insert("label".to_string(), vec!["jeryu.managed=true".to_string()]);

        let opts = ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        };

        let containers = self
            .docker
            .list_containers(Some(opts))
            .await
            .context("listing managed containers")?;
        Ok(containers)
    }

    /// Return full Docker IDs for jeryu-managed runner containers that are actually running.
    pub async fn running_managed_container_ids(&self) -> Result<BTreeSet<String>> {
        let ids = self
            .list_managed_containers()
            .await?
            .into_iter()
            .filter(|container| container.state.as_deref() == Some("running"))
            .filter_map(|container| container.id)
            .collect();
        Ok(ids)
    }

    // -- Events ------------------------------------------------------------

    pub fn events(
        &self,
    ) -> impl futures_util::Stream<Item = Result<bollard::models::EventMessage, bollard::errors::Error>>
    {
        self.docker
            .events(None::<bollard::system::EventsOptions<String>>)
    }

    // -- Compose (shell out) -----------------------------------------------

    pub async fn compose_up(&self) -> Result<()> {
        compose_up(self).await
    }

    pub async fn compose_up_service(&self, service: &str) -> Result<()> {
        compose_up_service(self, service).await
    }

    pub async fn compose_down(&self) -> Result<()> {
        compose_down(self).await
    }
}
