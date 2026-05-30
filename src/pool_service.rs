//! Owner: Runner Fleet / Pool Service
//! Proof: `cargo test -p jeryu --lib pool_service`
//! Invariants: orchestration handles stay in the library layer; the CLI only formats results.

use anyhow::Result;

use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::pool;
use crate::state::Db;

#[derive(Debug, Clone)]
pub struct PoolListRow {
    pub name: String,
    pub paused: bool,
    pub executor: String,
    pub min_warm: i64,
    pub live_managers: i64,
    pub db_managers: i64,
    pub max_managers: i64,
    pub gitlab_runner_id: i64,
}

impl PoolListRow {
    pub fn manager_status(&self) -> String {
        format!("{}/{}/{}", self.live_managers, self.db_managers, self.max_managers)
    }
}

pub struct PoolService {
    db: Db,
    docker: DockerCtl,
    client: GitlabClient,
}

impl PoolService {
    pub fn new(db: Db, docker: DockerCtl, client: GitlabClient) -> Self {
        Self { db, docker, client }
    }

    pub async fn connect(client: GitlabClient) -> Result<Self> {
        let db = Db::open().await?;
        let docker = DockerCtl::connect()?;
        Ok(Self::new(db, docker, client))
    }

    pub async fn list(&self) -> Result<Vec<PoolListRow>> {
        let pools = self.db.list_pools().await?;
        let mut rows = Vec::with_capacity(pools.len());

        for pool in pools {
            let db_managers = self.db.count_active_managers(&pool.name).await.unwrap_or(0);
            let live_managers = pool::count_running_managers(&self.db, &self.docker, &pool.name)
                .await
                .unwrap_or(0);
            rows.push(PoolListRow {
                name: pool.name,
                paused: pool.paused,
                executor: pool.executor,
                min_warm: pool.min_warm,
                live_managers,
                db_managers,
                max_managers: pool.max_managers,
                gitlab_runner_id: pool.gitlab_runner_id,
            });
        }

        Ok(rows)
    }

    pub async fn doctor(&self) -> Result<pool::PoolDoctorReport> {
        pool::build_pool_doctor_report(&self.db, &self.docker, &self.client).await
    }

    pub async fn repair(
        &self,
        options: pool::PoolRepairOptions,
    ) -> Result<pool::PoolRepairReport> {
        pool::repair_pool_state(&self.db, &self.docker, &self.client, options).await
    }

    pub async fn scale(&self, name: &str, count: usize) -> Result<usize> {
        pool::scale_pool_to(&self.db, &self.docker, &self.client, name, count).await
    }

    pub async fn pause(&self, name: &str) -> Result<()> {
        pool::pause_pool(&self.db, &self.client, name).await
    }

    pub async fn resume(&self, name: &str) -> Result<()> {
        pool::resume_pool(&self.db, &self.client, name).await
    }

    pub async fn drain(&self, name: &str) -> Result<()> {
        pool::drain_pool(&self.db, &self.docker, &self.client, name).await
    }

    pub async fn delete(&self, name: &str) -> Result<()> {
        pool::delete_pool(&self.db, &self.docker, &self.client, name).await
    }

    pub async fn rotate_token(&self, name: &str) -> Result<String> {
        pool::rotate_pool_token(&self.db, &self.docker, &self.client, name).await
    }
}
