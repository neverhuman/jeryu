use super::*;
use tracing::{info, warn};

impl GitlabClient {
    pub async fn list_all_runners(&self) -> Result<Vec<RunnerInfo>> {
        self.get_paginated_json("/runners/all").await
    }

    pub async fn get_runner(&self, runner_id: i64) -> Result<RunnerInfo> {
        self.api_get_json(self.api_url(&format!("/runners/{}", runner_id)))
            .await
            .context("get runner")
    }

    pub async fn list_all_runner_details(&self) -> Result<Vec<RunnerInfo>> {
        let runners = self.list_all_runners().await?;
        let mut details = Vec::with_capacity(runners.len());
        for runner in runners {
            match self.get_runner(runner.id).await {
                Ok(detail) => details.push(detail),
                Err(err) => {
                    warn!(
                        runner_id = runner.id,
                        error = %err,
                        "failed to fetch runner detail; falling back to list payload"
                    );
                    details.push(runner);
                }
            }
        }
        Ok(details)
    }

    pub async fn create_runner(
        &self,
        description: &str,
        tag_list: &[&str],
        run_untagged: bool,
        runner_type: &str,
    ) -> Result<RunnerCreated> {
        let resp: RunnerCreated = self
            .api_post_json(
                self.api_url("/user/runners"),
                &CreateRunnerReq {
                    description,
                    tag_list,
                    run_untagged,
                    runner_type,
                },
            )
            .await
            .context("create runner")?;
        info!(id = resp.id, "created runner");
        Ok(resp)
    }

    pub async fn set_runner_paused(&self, runner_id: i64, paused: bool) -> Result<()> {
        self.api_put_void(
            self.api_url(&format!("/runners/{}", runner_id)),
            &SetPausedReq { paused },
        )
        .await
        .context("set runner paused")?;
        info!(runner_id, paused, "updated runner paused state");
        Ok(())
    }

    pub async fn update_runner(
        &self,
        runner_id: i64,
        tag_list: &[&str],
        run_untagged: bool,
    ) -> Result<()> {
        self.api_put_void(
            self.api_url(&format!("/runners/{}", runner_id)),
            &UpdateRunnerReq {
                tag_list: Some(tag_list),
                run_untagged: Some(run_untagged),
            },
        )
        .await
        .context("update runner")?;
        info!(
            runner_id,
            run_untagged,
            tag_count = tag_list.len(),
            "updated runner tags"
        );
        Ok(())
    }

    pub async fn list_runner_managers(&self, runner_id: i64) -> Result<Vec<RunnerManager>> {
        let managers = self
            .api_get_json(self.api_url(&format!("/runners/{}/managers", runner_id)))
            .await?;
        Ok(managers)
    }

    pub async fn delete_runner(&self, runner_id: i64) -> Result<()> {
        self.api_delete_void(self.api_url(&format!("/runners/{}", runner_id)))
            .await?;
        info!(runner_id, "deleted runner");
        Ok(())
    }

    pub async fn reset_runner_token(&self, runner_id: i64) -> Result<String> {
        let resp: ResetTokenResp = self
            .api_post_nobody_json(self.api_url(&format!(
                "/runners/{}/reset_authentication_token",
                runner_id
            )))
            .await?;
        info!(runner_id, "reset runner auth token");
        Ok(resp.token)
    }
}
